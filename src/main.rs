extern crate regex;

use regex::Regex;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process;

fn remove_dead_links(dir: &Path) {
	if let Ok(entries) = fs::read_dir(dir) {
		for entry in entries.filter_map(Result::ok) {
			let path = entry.path();

			if path.is_dir() {
				remove_dead_links(&path);
			} else if path.is_symlink() {
				// Check if the symlink is dead
				if fs::read_link(&path).is_ok() && !path.exists() {
					println!("Removing dead symlink: {}", path.display());
					fs::remove_file(&path).expect(&format!(
						"Can't remove dead symlink: {}",
						path.display()
					));
				}
			}
		}
	}
}

fn visit_dirs(
    dir: &Path,
    source_base: &str,
    target: &str,
    ignore_git: bool,
    filter: bool,
    git_pattern: &Regex,
    ignore_patterns: &Vec<String>,
    force_overwrite: bool, // Add this parameter
) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let path_str = path.to_string_lossy().to_string();

            // Skip .git files/directories if ignore_git is true
            if ignore_git && git_pattern.is_match(&path_str) {
                continue;
            }

            // Generate relative name
            let name = if let Ok(stripped) = path.strip_prefix(source_base) {
                stripped.to_string_lossy().to_string()
            } else {
                continue;
            };

            // Handle filtering
            if filter {
                // Skip the .stow-local-ignore file itself
                if path_str.ends_with(".stow-local-ignore") {
                    continue;
                }

                // Remove leading slash if present
                let name_to_check = if name.starts_with('/') {
                    name[1..].to_string()
                } else {
                    name.clone()
                };

                // Check if this file/directory should be ignored
                let should_ignore = ignore_patterns
                    .iter()
                    .any(|pattern| name_to_check.starts_with(pattern));

                if should_ignore {
                    println!("Ignoring {}", path_str);
                    continue;
                }
            }

            let target_path = PathBuf::from(target).join(if name.starts_with('/') {
                &name[1..]
            } else {
                &name
            });

            if path.is_dir() {
                // Create target directory
                fs::create_dir_all(&target_path).expect(&format!(
                    "Impossibile creare la directory {}",
                    target_path.display()
                ));

                // Recursively visit subdirectories
                visit_dirs(
                    &path,
                    source_base,
                    target,
                    ignore_git,
                    filter,
                    git_pattern,
                    ignore_patterns,
                    force_overwrite,
                );
            } else if path.is_file() {
                // Create a symlink
                let canonical_path = fs::canonicalize(&path).expect(&format!(
                    "Can't get canonical path for {}",
                    path.display()
                ));

                // Handle existing files at the target path
                if target_path.exists() {
                    if target_path.is_symlink() {
                        // Always remove existing symlinks
                        fs::remove_file(&target_path).expect(&format!(
                            "Can't remove existing symlink: {}",
                            target_path.display()
                        ));
                    } else if force_overwrite {
                        // If it's a regular file and force_overwrite is enabled, backup and remove
                        let backup_path = format!("{}.bak", target_path.display());
                        println!("Backing up existing file to {}", backup_path);
                        fs::rename(&target_path, backup_path).expect(&format!(
                            "Can't backup existing file: {}",
                            target_path.display()
                        ));
                    } else {
                        // Skip if it's a regular file and force_overwrite is disabled
                        println!("Skipping existing file: {}", target_path.display());
                        continue;
                    }
                }

                // Create parent directory if necessary
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent).expect(&format!(
                        "Can't create parent directory {}",
                        parent.display()
                    ));
                }

                // Create the new symlink
                #[cfg(unix)]
                std::os::unix::fs::symlink(&canonical_path, &target_path).expect(&format!(
                    "Impossibile creare il symlink da {} a {}",
                    canonical_path.display(),
                    target_path.display()
                ));

                #[cfg(windows)]
                {
                    if canonical_path.is_dir() {
                        std::os::windows::fs::symlink_dir(&canonical_path, &target_path).expect(
                            &format!(
                                "Can't create symlink for directory from {} to {}",
                                canonical_path.display(),
                                target_path.display()
                            ),
                        );
                    } else {
                        std::os::windows::fs::symlink_file(&canonical_path, &target_path).expect(
                            &format!(
                                "Can't create symlink from {} to {}",
                                canonical_path.display(),
                                target_path.display()
                            ),
                        );
                    }
                }
            }
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} SOURCE DESTINATION [--force]", args[0]);
        process::exit(1);
    }

    let source = &args[1];
    let mut target = args[2].clone();
    
    let ignore_git = true;
    let mut filter = false;
    let force_overwrite = args.len() > 3 && args[3] == "--force";

    // Replace ~ with $HOME
    if target.starts_with("~") {
        let home = env::var("HOME").expect("Can't get HOME variable");
        target = target.replacen("~", &home, 1);
    }

    // Create destination directory if it doesn't exist
    fs::create_dir_all(&target).expect("Can't create destination directory");

    // Remove dead symlinks from destination directory
    remove_dead_links(Path::new(&target));

    // Check if .stow-local-ignore file exists
    let stow_ignore_path = Path::new(source).join(".stow-local-ignore");
    if stow_ignore_path.exists() {
        filter = true;
    }

    // Read ignore patterns
    let ignore_patterns = if filter {
        let file = fs::File::open(&stow_ignore_path)
            .expect("Can't open .stow-local-ignore");
        let reader = BufReader::new(file);
        reader
            .lines()
            .filter_map(Result::ok)
            .collect::<Vec<String>>()
    } else {
        vec![]
    };

    // Compile regex pattern for .git
    let git_pattern = Regex::new(r"\.git(/|$)").expect("Invalid regex");

    // Visit directories
    visit_dirs(
        Path::new(source),
        source,
        &target,
        ignore_git,
        filter,
        &git_pattern,
        &ignore_patterns,
        force_overwrite,
    );
}
