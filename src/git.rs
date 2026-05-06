use std::{path::Path, sync::Arc};

use async_tempfile::TempDir;
use git2::{Cred, CredentialType, IndexAddOption, Oid, Remote, RemoteCallbacks};
use thiserror::Error;

use crate::AppState;

#[derive(Debug, Error)]
pub enum CloneError {
  #[error("git operation failed during clone: {0}")]
  Git2(#[from] git2::Error),
  
  #[error("tempfile operation failed during clone: {0}")]
  TempDir(#[from] async_tempfile::Error)
}

/// Provides Git remote callbacks for authentication.
///
/// For SSH authentication, will provide the path to the configured SSH private key. If none is found,
/// this authentication will fail. If a specific username is configured for SSH, it will be used. Otherwise,
/// this authentication will attempt to extract the username from the SSH url.
/// 
/// For HTTP(s) authentication, will provide the configured username and password. If none is found,
/// this authentication will fail.
pub fn get_remote_callbacks(state: &Arc<AppState>) -> RemoteCallbacks<'_> {
    let git = &state.config.micropub.content.git;

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_url, username_from_url, r#type| {
        let username = git
            .username
            .as_deref()
            .or(username_from_url)
            .ok_or_else(|| git2::Error::from_str("no git username available"))?;

        if r#type.contains(CredentialType::USERNAME) {
            return Cred::username(username);
        } else if r#type.contains(CredentialType::SSH_KEY) {
            let key_path = git
                .key_path
                .as_deref()
                .map(|s| Path::new(s))
                .filter(|p| p.is_file())
                .ok_or_else(|| git2::Error::from_str("failed to resolve ssh private key"))?;

            return Cred::ssh_key(username, None, key_path, git.password.as_deref());
        } else if r#type.contains(CredentialType::USER_PASS_PLAINTEXT) {
            return Cred::userpass_plaintext(
                username,
                git.password
                    .as_deref()
                    .ok_or_else(|| git2::Error::from_str("no git password found"))?,
            );
        }

        return Err(git2::Error::from_str("unsupported credential type"));
    });

    callbacks
}

/// Clones the configured repository into a temporary directory.
/// The directory will be deleted when the TempDir goes out of scope.
pub async fn clone_repo(state: &Arc<AppState>) -> Result<(git2::Repository, TempDir), CloneError> {
    let location = TempDir::new().await.map_err(|e| CloneError::TempDir(e))?;

    let mut fo = git2::FetchOptions::new();
    fo.remote_callbacks(get_remote_callbacks(state));
    fo.depth(1);

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fo);
    
    let repo = builder.clone(
      &state.config.micropub.content.git.repository, 
      location.dir_path()
    ).map_err(|e| CloneError::Git2(e))?;

    Ok((repo, location))
}

/// Adds a particular path to the repository index (i.e., stages the path for commit)
pub fn add_path(repo: &git2::Repository, path: &str) -> Result<git2::Oid, git2::Error> {
  let mut idx = repo.index()?;
  idx.add_path(Path::new(&path))?;
  idx.write()?;
  idx.write_tree()
}

// Adds all changes to the repository index (i.e., stages everything for commit)
pub fn add_all(repo: &git2::Repository) -> Result<git2::Oid, git2::Error> {
  let mut idx = repo.index()?;
  idx.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
  idx.write()?;
  idx.write_tree()
}

/// Commits a particular object ID (oid) to prepare for pushing. The Oid is
/// returned from `add_path`.
pub fn commit(repo: &git2::Repository, oid: Oid, message: &str) -> Result<(), git2::Error> {
  let tree = repo.find_tree(oid)?;

  let parents = match repo.head() {
    Ok(head) => vec![head.peel_to_commit()?],
    Err(_) => vec![]
  };

  let parent_refs: Vec<&git2::Commit<'_>> = parents.iter().collect();

  let sig = git2::Signature::now("scribble", "scribble@indieweb")?;
  let _ = repo.commit(
    Some("HEAD"), 
    &sig, 
    &sig, 
    &format!("scribble: {}", message), 
    &tree,
    &parent_refs
  )?;

  Ok(())
}

/// Adds a particular path to the repository index, and then immediately commits it.
pub fn add_and_commit(repo: &git2::Repository, path: &str, message: &str) -> Result<(), git2::Error> {
  let oid = add_path(&repo, path)?;
  commit(repo, oid, message)?;
  Ok(())
}

// Adds all changes to the repository index, and then immediately commits them.
pub fn add_all_and_commit(repo: &git2::Repository, message: &str) -> Result<(), git2::Error> {
  let oid = add_all(repo)?;
  commit(repo, oid, message)?;
  Ok(())
} 

/// Pushes a branch of the repository to its remote equivalent.
pub fn push(state: &Arc<AppState>, repo: &git2::Repository, branch: &str) -> Result<(), git2::Error> {
  let mut remote = repo.find_remote("origin")?;

  let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

  let mut po = git2::PushOptions::new();
  po.remote_callbacks(get_remote_callbacks(state));

  remote.push(&[&refspec], Some(&mut po))?;

  Ok(())
}

/// Attempts to connect to the configured repository, using the configured credentials.
/// This is used as an initial healthcheck before finalizing startup.
pub fn try_connect_repo(state: &Arc<AppState>) -> Result<(), git2::Error> {
    let mut rem = Remote::create_detached(state.config.micropub.content.git.repository.clone())?;

    rem.connect_auth(
        git2::Direction::Fetch,
        Some(get_remote_callbacks(state)),
        None,
    )?;

    rem.disconnect()?;

    Ok(())
}
