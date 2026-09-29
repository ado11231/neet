use std::path::Path;
use walkdir::WalkDir;

/// Walks a directory without following links or crossing filesystem boundaries
pub fn walk_directory(root: &Path) -> impl Iterator<Item = walkdir::Result<walkdir::DirEntry>> {
    WalkDir::new(root)
        .follow_links(false)
        .same_file_system(true)
        .into_iter()
}

#[cfg(test)]
mod tests {
    use super::walk_directory;
    use std::fs;
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    #[test]
    fn walks_a_directory_and_files() {
        let root = tempdir().expect("temporary directory should be created");
        let file_path = root.path().join("example.txt");

        fs::write(&file_path, "example").expect("test file should be written");

        let paths = walk_directory(root.path())
            .map(|result| {
                result
                    .expect("directory entry should be readable")
                    .into_path()
            })
            .collect::<Vec<_>>();

        assert!(paths.contains(&root.path().to_path_buf()));
        assert!(paths.contains(&file_path));
    }

    #[test]
    fn reports_a_symlink_without_following_it() {
        let root = tempdir().expect("scan directory should be created");
        let target = tempdir().expect("target directory should be created");

        let target_file = target.path().join("inside.txt");
        fs::write(&target_file, "example").expect("target file should be written");

        let link_path = root.path().join("linked-directory");
        symlink(target.path(), &link_path).expect("symbolic link should be created");

        let paths = walk_directory(root.path())
            .map(|result| {
                result
                    .expect("directory entry should be readable")
                    .into_path()
            })
            .collect::<Vec<_>>();

        assert!(paths.contains(&link_path));
        assert!(!paths.contains(&link_path.join("inside.txt")));
    }
    #[test]
    fn reports_the_path_for_a_missing_root() {
        let directory = tempdir().expect("temporary directory should be created");
        let missing_path = directory.path().join("missing");

        let result = walk_directory(&missing_path)
            .next()
            .expect("walker should return one result");

        let error = result.expect_err("missing root should produce an error");

        assert_eq!(error.path(), Some(missing_path.as_path()));
    }
}
