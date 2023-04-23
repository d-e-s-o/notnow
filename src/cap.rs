// Copyright (C) 2022 Daniel Mueller (deso@posteo.net)
// SPDX-License-Identifier: GPL-3.0-or-later

//! A module providing the means for protecting the contents of a
//! directory from external modification while at the same time allowing
//! for vetted access from within the program itself.

use std::ffi::OsStr;
use std::fs::Permissions;
use std::future::Future;
use std::io::ErrorKind;
use std::marker::PhantomData;
use std::ops::Deref;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::path::PathBuf;
use std::thread;

use anyhow::Context as _;
use anyhow::Result;

use libc::S_IWUSR;

use tokio::fs::metadata;
use tokio::fs::read_dir;
use tokio::fs::set_permissions;
use tokio::runtime::Handle;


/// Change the provided `Permissions` by making it read-only.
#[cfg(unix)]
fn read_only(mut permissions: Permissions) -> Permissions {
  // Remove user write permissions.
  let () = permissions.set_mode(permissions.mode() & !S_IWUSR);
  permissions
}

/// Change the provided `Permissions` by removing the write permission.
#[cfg(not(unix))]
fn read_only(mut permissions: Permissions) -> Permissions {
  let () = permissions.set_readonly(true);
  permissions
}

/// Change the provided mode by adding the user-write permission.
#[cfg(unix)]
fn writeable(mut permissions: Permissions) -> Permissions {
  // Set user write permissions.
  let () = permissions.set_mode(permissions.mode() | S_IWUSR);
  permissions
}

/// Change the provided mode by making it not read-only.
#[cfg(not(unix))]
fn writeable(mut permissions: Permissions) -> Permissions {
  let () = permissions.set_readonly(false);
  permissions
}


/// Change the permissions of the "item" (typically: file or
/// directory) represented by the provided path.
///
/// # Notes
/// This function, by design, succeeds without doing anything if the
/// provided path does not exist.
async fn change_item_permissions<F>(path: &Path, f: F) -> Result<()>
where
  F: FnOnce(Permissions) -> Permissions,
{
  let meta_data = match metadata(path).await {
    Ok(meta_data) => meta_data,
    Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
    r @ Err(_) => {
      r.with_context(|| format!("failed to retrieve meta data for {}", path.display()))?
    },
  };

  // We are explicitly working with the mode here, because using
  // Permissions::set_readonly() will not only adjust the user's but
  // also everybody else's rights on the file, which is not something
  // we want.
  let permissions = meta_data.permissions();
  let new_permissions = f(permissions.clone());

  if new_permissions != permissions {
    let () = set_permissions(path, new_permissions)
      .await
      .with_context(|| format!("failed to adjust permissions of {}", path.display()))?;
  }
  Ok(())
}

/// Change the permissions of the provided directory and the files in
/// it.
///
/// # Notes
/// This function does not recurse, i.e., only files directly contained
/// in the provided directory are affected, not those in sub-directories.
async fn change_directory_permissions<F>(directory: &Path, f: F) -> Result<()>
where
  F: Fn(Permissions) -> Permissions,
{
  change_item_permissions(directory, &f).await?;

  let mut dir = match read_dir(directory).await {
    Ok(dir) => dir,
    Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
    r @ Err(_) => r.with_context(|| {
      format!(
        "failed to read contents of directory {}",
        directory.display()
      )
    })?,
  };

  while let Some(entry) = dir.next_entry().await.with_context(|| {
    format!(
      "failed to iterate contents of directory {}",
      directory.display()
    )
  })? {
    let file_type = entry
      .file_type()
      .await
      .with_context(|| format!("failed to inquire file type of {}", entry.path().display()))?;

    // Because we are also not recursing, it seems consequent to ignore
    // directories altogether.
    if !file_type.is_dir() {
      let () = change_item_permissions(&entry.path(), &f).await?;
    }
  }
  Ok(())
}


/// Run a future from a synchronous context, blocking until it is
/// resolved.
fn run_async<Fut>(future: Fut)
where
  Fut: Future<Output = ()> + Send,
{
  // In order to be able to run async functionality from a sync context
  // (such as a `Drop` impl), we grab a handle to the async runtime that
  // must have been started and then spawn our future on it, in a new
  // thread (the new thread is necessary because...Tokio).
  // This theater is necessary to not litter Tokio specifics throughout
  // the code base.
  let handle = Handle::current();

  let () = thread::scope(|scope| {
    let handle = scope.spawn(|| handle.block_on(future));
    // SANITY: We panic if the future caused the thread to panic. But
    //         really we just emulate a synchronous call.
    handle.join().unwrap()
  });
}


/// A capability to a directory.
///
/// An object of this type "protects" a directory and its contents,
/// while providing on demand access to it. In a nutshell, on creation
/// it adjusts the directory's permissions, removing write access from
/// it and all files.
///
/// When a client wants to create a new file or modify an existing one,
/// write access can be granted through the [`write`][DirCap::write]
/// method and the returned [`WriteGuard`]'s
/// [`file_cap`][WriteGuard::file_cap] method. Once the `WriteGuard`
/// leaves the scope, the directory will become read-only again.
///
/// It is only when the `DirCap` object itself is destroyed that write
/// access to the directory and its contents is restored for good.
///
/// # Notes
/// By design this type handles a non-existent directory gracefully.
#[derive(Debug)]
pub struct DirCap {
  /// The directory being "protected" by this capability.
  directory: PathBuf,
}

impl DirCap {
  /// Create a [`DirCap`] object for the provided directory.
  pub async fn for_dir(directory: PathBuf) -> Result<Self> {
    let slf = Self { directory };
    let () = slf.protect().await?;

    Ok(slf)
  }

  /// Protect the directory and the files contained in it from
  /// modification.
  async fn protect(&self) -> Result<()> {
    change_directory_permissions(&self.directory, read_only).await
  }

  /// Restore writable state of the directory and files contained in it.
  async fn unprotect(&self) -> Result<()> {
    change_directory_permissions(&self.directory, writeable).await
  }

  /// Open the directory to write operations.
  pub async fn write(&mut self) -> Result<WriteGuard<'_>> {
    WriteGuard::new(self).await
  }

  /// Retrieve the path to the directory referenced by this capability.
  pub fn path(&self) -> &Path {
    &self.directory
  }
}

impl Drop for DirCap {
  fn drop(&mut self) {
    let () = run_async(async {
      // We basically ignore errors here (except when assertions are
      // explicitly enabled). Being in a destructor we can't do much
      // about them anyway, but there are theoretical possibilities to
      // cope with them better, e.g., with a constructor similar to
      // [`std::thread::scope`]. However, such an approach does not lend
      // itself very well to the problem at hand and at the same time
      // even if the operation fails the program would self-correct next
      // time and as such the behavior is deemed acceptable.
      // TODO: If we ever add a logging solution we could consider at
      //       least logging an error, though.
      let result = self.unprotect().await;
      if cfg!(debug_assertions) {
        let () = result.unwrap_or_else(|error| {
          panic!(
            "failed to revert permissions of {}: {error}",
            self.directory.display()
          )
        });
      }
    });
  }
}


/// An object granting access to a directory.
#[derive(Debug)]
pub struct WriteGuard<'cap> {
  /// The underlying capability to a directory.
  dir_cap: &'cap mut DirCap,
}

impl<'cap> WriteGuard<'cap> {
  /// Create a new [`WriteGuard`] based on the given [`DirCap`].
  async fn new(dir_cap: &'cap mut DirCap) -> Result<WriteGuard<'cap>> {
    let () = change_item_permissions(&dir_cap.directory, writeable).await?;
    Ok(Self { dir_cap })
  }

  /// Retrieve a [`FileCap`] for the provided file.
  pub fn file_cap<'slf>(&'slf self, file: &OsStr) -> FileCap<'slf> {
    FileCap::new(self.dir_cap.directory.join(file))
  }
}

impl Deref for WriteGuard<'_> {
  type Target = DirCap;

  fn deref(&self) -> &Self::Target {
    self.dir_cap
  }
}

impl Drop for WriteGuard<'_> {
  fn drop(&mut self) {
    let () = run_async(async {
      // Note that we intentionally change permissions of the entire
      // directory here. The reason is that new files may have been
      // created and we'd want to make sure that they are
      // write-protected moving forward.
      let result = change_directory_permissions(&self.dir_cap.directory, read_only).await;
      if cfg!(debug_assertions) {
        let () = result.unwrap_or_else(|error| {
          panic!(
            "failed to revert permissions of {}: {error}",
            self.dir_cap.directory.display()
          )
        });
      }
    });
  }
}


/// A capability to doing something with a file.
#[derive(Debug)]
pub struct FileCap<'cap> {
  /// The path of the file which this capability refers to.
  path: PathBuf,
  /// Phantom data for the 'cap lifetime.
  _phantom: PhantomData<&'cap ()>,
}

impl<'cap> FileCap<'cap> {
  /// Create a new [`FileCap`] for the provided path.
  fn new(path: PathBuf) -> Self {
    Self {
      path,
      _phantom: PhantomData,
    }
  }

  /// Do something with the path represented by this [`FileCap`].
  pub async fn with_writeable_path<F, Fut>(&mut self, f: F) -> Result<()>
  where
    F: FnOnce(PathBuf) -> Fut,
    Fut: Future<Output = Result<()>>,
  {
    let () = change_item_permissions(&self.path, writeable).await?;

    let result = f(self.path.to_path_buf()).await;
    match (result, change_item_permissions(&self.path, read_only).await) {
      (Ok(()), Ok(())) => Ok(()),
      (Ok(()), r @ Err(_)) => {
        r.with_context(|| format!("failed to revert permissions of {}", self.path.display()))
      },
      (r @ Err(_), Ok(())) => r,
      (r @ Err(_), Err(_)) => {
        eprintln!("failed to revert permissions of {}", self.path.display());
        r
      },
    }
  }

  /// Retrieve the path to the file this capability refers to.
  pub fn path(&self) -> &Path {
    &self.path
  }
}


#[cfg(test)]
mod tests {
  use super::*;

  use tempfile::NamedTempFile;
  use tempfile::TempDir;

  use tokio::fs::create_dir;
  use tokio::fs::read_to_string;
  use tokio::fs::remove_file;
  use tokio::fs::write;
  use tokio::test;


  /// Make sure that a capability "protects" a provided directory.
  #[test]
  async fn protect_directory() {
    let root = TempDir::new().unwrap();
    let file1 = NamedTempFile::new_in(root.path()).unwrap();
    let file2 = NamedTempFile::new_in(root.path()).unwrap();
    let file3 = NamedTempFile::new_in(root.path()).unwrap();
    let file4 = NamedTempFile::new_in(root.path()).unwrap();

    {
      let path = root.path().to_path_buf();
      let _capability = DirCap::for_dir(path).await.unwrap();
      // While the capability is "active" we should not be able to
      // delete files in said directory.
      let error = remove_file(file1.path()).await.unwrap_err();
      assert_eq!(error.kind(), ErrorKind::PermissionDenied);

      let error = NamedTempFile::new_in(root.path()).unwrap_err();
      assert_eq!(error.kind(), ErrorKind::PermissionDenied);

      let error = write(file2.path(), "test data").await.unwrap_err();
      assert_eq!(error.kind(), ErrorKind::PermissionDenied);
    }

    // With the capability destroyed, we should again be able to work on
    // the above files.
    let () = write(file3.path(), "hihi, it works").await.unwrap();
    let () = remove_file(file4.path()).await.unwrap();

    // Also make sure that we can create a new file.
    let _file5 = NamedTempFile::new_in(root.path()).unwrap();
  }

  /// Check that our capability infrastructure works correctly for
  /// directories or files not yet present.
  #[test]
  async fn non_existent_directory_and_file() {
    let path = {
      let dir = TempDir::new().unwrap();
      dir.path().to_path_buf()
    };

    let mut capability = DirCap::for_dir(path.clone()).await.unwrap();
    let write_guard = capability.write().await.unwrap();
    let mut file_cap = write_guard.file_cap(OsStr::new("non-existent-file-in-non-existent-dir"));
    let () = file_cap
      .with_writeable_path(|path| async move {
        assert!(!path.exists());
        Ok(())
      })
      .await
      .unwrap();

    assert!(!path.exists());
  }

  /// Make sure that a newly created directory is retroactively
  /// protected after relinquishing write access through the capability.
  #[test]
  async fn newly_created_directory_is_protected() {
    let path = {
      let dir = TempDir::new().unwrap();
      dir.path().to_path_buf()
    };

    let mut capability = DirCap::for_dir(path.clone()).await.unwrap();
    {
      let _write_guard = capability.write().await.unwrap();
      let () = create_dir(&path).await.unwrap();

      // We should have write access to the newly created directory.
      let () = write(path.join("test-file"), "test data").await.unwrap();
    }

    // The newly created directory should have been "retroactively"
    // write-protected once the write guard went out of scope.
    let error = write(path.join("another-file"), "test").await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::PermissionDenied);
  }

  /// Check that we can use a [`FileCap`] to open a file for
  /// modification.
  #[test]
  async fn newly_created_file_is_protected() {
    let root = TempDir::new().unwrap();
    let path = root.path().to_path_buf();
    let mut capability = DirCap::for_dir(path).await.unwrap();

    let path = {
      let _guard = capability.write().await.unwrap();

      // At this point we should be able to create new files.
      let file = NamedTempFile::new_in(root.path()).unwrap();
      let path = file.into_temp_path().keep().unwrap();
      path
    };

    // The file should now be write-protected.
    let error = write(path, "test data").await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::PermissionDenied);
  }

  /// Check that we can use a [`FileCap`] to open a file for
  /// modification.
  #[test]
  async fn file_cap_unprotects_file() {
    let root = TempDir::new().unwrap();
    let file = NamedTempFile::new_in(root.path()).unwrap();

    {
      let path = root.path().to_path_buf();
      let mut capability = DirCap::for_dir(path).await.unwrap();
      let guard = capability.write().await.unwrap();

      let mut file_cap = guard.file_cap(file.path().file_name().unwrap());

      // We have the capability, but if we are not using it we should
      // still be denied permission to write.
      let error = write(file.path(), "test data").await.unwrap_err();
      assert_eq!(error.kind(), ErrorKind::PermissionDenied);

      let () = file_cap
        .with_writeable_path(|path| async move {
          let () = write(path, "success").await.unwrap();
          Ok(())
        })
        .await
        .unwrap();

      // Outside of the specific call we should not be able to write.
      let error = write(file.path(), "test data").await.unwrap_err();
      assert_eq!(error.kind(), ErrorKind::PermissionDenied);
    }

    let content = read_to_string(file.path()).await.unwrap();
    assert_eq!(content, "success");
  }
}
