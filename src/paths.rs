// Copyright (C) 2024 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

use std::borrow::Cow;
use std::env::current_dir;
use std::ffi::OsStr;
use std::fs::canonicalize;
use std::io;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt as _;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Context as _;
use anyhow::Result;

use dirs::cache_dir;
use dirs::config_dir;


/// Normalize a path, removing current and parent directory components
/// (if possible).
// Compared to Cargo's "reference" implementation
// https://github.com/rust-lang/cargo/blob/fede83ccf973457de319ba6fa0e36ead454d2e20/src/cargo/util/paths.rs#L61
// we correctly handle something like '../x' (by leaving it alone). On
// the downside, we can end up with '..' components unresolved, if they
// are at the beginning of the path.
fn normalize(path: &Path) -> PathBuf {
  let components = path.components();
  let path = PathBuf::with_capacity(path.as_os_str().len());

  let mut path = components.fold(path, |mut path, component| {
    match component {
      Component::Prefix(..) | Component::RootDir => (),
      Component::CurDir => return path,
      Component::ParentDir => {
        if let Some(prev) = path.components().next_back() {
          match prev {
            Component::CurDir => {
              // SANITY: We can never have a current directory component
              //         inside `path` because we never added one to
              //         begin with.
              unreachable!()
            },
            Component::Prefix(..) | Component::RootDir | Component::ParentDir => (),
            Component::Normal(..) => {
              path.pop();
              return path
            },
          }
        }
      },
      Component::Normal(c) => {
        path.push(c);
        return path
      },
    }

    path.push(component.as_os_str());
    path
  });

  let () = path.shrink_to_fit();
  path
}


/// Perform best-effort canonicalization on the provided path.
///
/// Path components that do not exist do not cause the function to fail
/// and will be included in the result, with only normalization
/// performed on them.
fn canonicalize_non_strict(path: &Path) -> io::Result<PathBuf> {
  let mut path = path;
  let input = path;

  let resolved = loop {
    match canonicalize(path) {
      Ok(resolved) => break Cow::Owned(resolved),
      Err(err) if err.kind() == ErrorKind::NotFound => (),
      e => return e,
    }

    match path.parent() {
      None => {
        // We have reached the root. No point in attempting to
        // canonicalize further. We are done.
        path = Path::new("");
        break Cow::Borrowed(path)
      },
      Some(parent) if parent == Path::new("") => {
        // `path` is a relative path with a single component, so resolve
        // it to the current directory.
        path = parent;
        break Cow::Owned(current_dir()?)
      },
      Some(parent) => {
        // We need a bit of a dance here in order to get the parent path
        // but including the trailing path separator. That's necessary
        // for our path "subtraction" below to work correctly.
        let parent_len = parent.as_os_str().as_bytes().len();
        let path_bytes = path.as_os_str().as_bytes();
        // SANITY: We know that `path` has a parent (a true substring).
        //         Given that we are dealing with paths, we also know
        //         that a trailing path separator *must* exist, meaning
        //         we will always be in bounds.
        path = Path::new(OsStr::from_bytes(
          path_bytes
            .get(parent_len + 1..)
            .expect("constructed path has no trailing separator"),
        ));
      },
    }
  };

  let input_bytes = input.as_os_str().as_bytes();
  let path_len = path.as_os_str().as_bytes().len();
  // SANITY: We know that `path` is a substring of `input` and so we can
  //         never be out-of-bounds here.
  let unresolved = input_bytes
    .get(path_len..)
    .expect("failed to access input path sub-string");
  let complete = resolved.join(OsStr::from_bytes(unresolved));
  // We need to make sure to normalize the result here, because while
  // the unresolved part does not actually exist on the file system, it
  // could still contain symbolic references to the current or parent
  // directory that we do not want in the result.
  let normalized = normalize(&complete);
  Ok(normalized)
}


/// A type taking care of the program's path handling needs.
#[derive(Debug)]
pub struct Paths {
  /// The path to the configuration directory.
  ///
  /// This path will always be normalized.
  config_dir: PathBuf,
  /// The path to the directory containing "ephemeral" state.
  state_dir: PathBuf,
}

impl Paths {
  /// Instantiate a new `Paths` object, optionally using `config_dir` as
  /// the directory storing configuration data (including tasks).
  pub fn new(config_dir: Option<PathBuf>) -> Result<Self> {
    let config_dir = if let Some(config_dir) = config_dir {
      config_dir
    } else {
      self::config_dir()
        .ok_or_else(|| anyhow!("unable to determine config directory"))?
        .join("notnow")
    };
    let config_dir = canonicalize_non_strict(&config_dir)
      .with_context(|| format!("failed to canonicalize path `{}`", config_dir.display()))?;

    let mut config_dir_rel = config_dir.components();
    let _root = config_dir_rel.next();
    let config_dir_rel = config_dir_rel.as_path();
    debug_assert!(config_dir_rel.is_relative(), "{config_dir_rel:?}");

    let state_dir = cache_dir()
      .ok_or_else(|| anyhow!("unable to determine cache directory"))?
      .join("notnow")
      .join(config_dir_rel);

    let slf = Self {
      config_dir,
      state_dir,
    };
    Ok(slf)
  }

  /// Retrieve the path to the program's configuration directory.
  pub fn ui_config_dir(&self) -> &Path {
    &self.config_dir
  }

  /// Retrieve the file name of the program's UI configuration.
  pub fn ui_config_file(&self) -> &OsStr {
    OsStr::new("notnow.json")
  }

  /// Retrieve the path to the program's task directory.
  pub fn tasks_dir(&self) -> PathBuf {
    self.ui_config_dir().join("tasks")
  }

  /// Retrieve the path to the program's "volatile" UI state directory.
  pub fn ui_state_dir(&self) -> &Path {
    &self.state_dir
  }

  /// Retrieve the file name of the program's "volatile" UI state.
  pub fn ui_state_file(&self) -> &OsStr {
    OsStr::new("ui-state.json")
  }

  /// Retrieve the path to the program's lock file.
  pub(crate) fn lock_file(&self) -> PathBuf {
    self.state_dir.join("notnow.lock")
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use tempfile::TempDir;


  /// Check that we can normalize paths as expected.
  #[test]
  fn path_normalization() {
    assert_eq!(normalize(Path::new("tmp/foobar/..")), Path::new("tmp"));
    assert_eq!(normalize(Path::new("/tmp/foobar/..")), Path::new("/tmp"));
    assert_eq!(normalize(Path::new("/tmp/.")), Path::new("/tmp"));
    assert_eq!(normalize(Path::new("/tmp/./blah")), Path::new("/tmp/blah"));
    assert_eq!(normalize(Path::new("/tmp/../blah")), Path::new("/blah"));
    assert_eq!(normalize(Path::new("./foo")), Path::new("foo"));
    assert_eq!(
      normalize(Path::new("./foo/")).as_os_str(),
      Path::new("foo").as_os_str()
    );
    assert_eq!(normalize(Path::new("foo")), Path::new("foo"));
    assert_eq!(
      normalize(Path::new("foo/")).as_os_str(),
      Path::new("foo").as_os_str()
    );
    assert_eq!(normalize(Path::new("../foo")), Path::new("../foo"));
    assert_eq!(normalize(Path::new("../foo/")), Path::new("../foo"));
    assert_eq!(
      normalize(Path::new("./././relative-dir-that-does-not-exist/../file")),
      Path::new("file")
    );
  }

  /// Test that we can canonicalize paths on a best-effort basis.
  #[test]
  fn non_strict_canonicalization() {
    let dir = current_dir().unwrap();
    let path = Path::new("relative-path-that-does-not-exist");
    let real = canonicalize_non_strict(path).unwrap();
    assert_eq!(real, dir.join(path));

    let dir = current_dir().unwrap();
    let path = Path::new("relative-path-that-does-not-exist/");
    let real = canonicalize_non_strict(path).unwrap();
    assert_eq!(
      real.as_os_str(),
      dir
        .join(Path::new("relative-path-that-does-not-exist"))
        .as_os_str()
    );

    let path = Path::new("relative-dir-that-does-not-exist/file");
    let real = canonicalize_non_strict(path).unwrap();
    assert_eq!(real, dir.join(path));

    let path = Path::new("./relative-dir-that-does-not-exist/file");
    let real = canonicalize_non_strict(path).unwrap();
    assert_eq!(real, dir.join(normalize(path)));

    let path = Path::new("./././relative-dir-that-does-not-exist/../file");
    let real = canonicalize_non_strict(path).unwrap();
    assert_eq!(real, dir.join("file"));

    let path = Path::new("../relative-path-that-does-not-exist");
    let real = canonicalize_non_strict(path).unwrap();
    assert_eq!(
      real,
      dir
        .parent()
        .unwrap()
        .join("relative-path-that-does-not-exist")
    );

    let path = Path::new("../relative-dir-that-does-not-exist/file");
    let real = canonicalize_non_strict(path).unwrap();
    assert_eq!(
      real,
      dir
        .parent()
        .unwrap()
        .join("relative-dir-that-does-not-exist/file")
    );

    let path = Path::new("/absolute-path-that-does-not-exist");
    let real = canonicalize_non_strict(path).unwrap();
    assert_eq!(real, path);

    let path = Path::new("/absolute-dir-that-does-not-exist/file");
    let real = canonicalize_non_strict(path).unwrap();
    assert_eq!(real, path);

    let dir = TempDir::new().unwrap();
    let dir = dir.path();

    let path = dir;
    let real = canonicalize_non_strict(path).unwrap();
    assert_eq!(real, path);

    let path = dir.join("foobar");
    let real = canonicalize_non_strict(&path).unwrap();
    assert_eq!(real, path);
  }

  /// Make sure that we can instantiate a `Paths` object properly.
  #[test]
  fn paths_instantiation() {
    let _paths = Paths::new(None).unwrap();

    let dir = TempDir::new().unwrap();
    let path = dir.path().join("i").join("do").join("not").join("exist");
    let _paths = Paths::new(Some(path)).unwrap();
  }
}
