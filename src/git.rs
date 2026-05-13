use std::{path::Path, sync::Arc};

use async_tempfile::TempDir;
use git2::{Cred, CredentialType, IndexAddOption, Oid, RemoteCallbacks, Status};
use thiserror::Error;
use tracing::instrument;

use crate::{AppState, config::MicropubContentGit};

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
pub fn get_remote_callbacks(config: &MicropubContentGit) -> RemoteCallbacks<'_> {
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_url, username_from_url, r#type| {
        let username = config
            .username
            .as_deref()
            .or(username_from_url)
            .ok_or_else(|| git2::Error::from_str("no git username available"))?;

        if r#type.contains(CredentialType::USERNAME) {
            return Cred::username(username);
        } else if r#type.contains(CredentialType::SSH_KEY) {
            let key_path = config
                .key_path
                .as_deref()
                .map(|s| Path::new(s))
                .filter(|p| p.is_file())
                .ok_or_else(|| git2::Error::from_str("failed to resolve ssh private key"))?;

            return Cred::ssh_key(username, None, key_path, config.password.as_deref());
        } else if r#type.contains(CredentialType::USER_PASS_PLAINTEXT) {
            return Cred::userpass_plaintext(
                username,
                config.password
                    .as_deref()
                    .ok_or_else(|| git2::Error::from_str("no git password found"))?,
            );
        }

        return Err(git2::Error::from_str("unsupported credential type"));
    });

    callbacks
}

fn get_fetch_options(config: &MicropubContentGit) -> git2::FetchOptions<'_> {
    let mut fo = git2::FetchOptions::new();
    fo.remote_callbacks(get_remote_callbacks(config));
    // fo.depth(1);
    fo
}

/// Uses the provided git configuration to clones the repository into the provided 
/// temporary directory.
#[instrument]
pub fn clone_repo(config: &MicropubContentGit, location: &TempDir) -> Result<git2::Repository, CloneError> {
    let fo = get_fetch_options(config);

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fo);
    
    let repo = builder.clone(
      &config.repository,
      location.dir_path()
    ).map_err(|e| CloneError::Git2(e))?;

    Ok(repo)
}

/// Fetches the remote and updates the local repository with any upstream changes
#[instrument(skip(repo))]
fn fetch_repo(config: &MicropubContentGit, repo: &git2::Repository) -> Result<(), git2::Error> {
  let mut remote = repo.find_remote("origin")?;
  let mut fo = get_fetch_options(config);
  let branch = config.branch.clone().unwrap_or(String::from("main"));
  remote.fetch(&[&branch], Some(&mut fo), None)?;
  Ok(())
}

/// Hard resets the local repo to match the remote, throwing away anything else
#[instrument(skip(repo))]
fn reset_to_head(config: &MicropubContentGit, repo: &git2::Repository) -> Result<(), git2::Error> {
  let branch = config.branch.clone().unwrap_or(String::from("main"));
  let origin_branch = repo.find_reference(&format!("refs/remotes/origin/{branch}"))?;
  let commit = origin_branch.peel_to_commit()?;
  repo.reset(commit.as_object(), git2::ResetType::Hard, None)?;
  Ok(())
}

/// Cleans any unstaged files from the local repo to prepare for new work
#[instrument(skip(repo))]
pub async fn clean_repo(repo: &git2::Repository) -> Result<(), git2::Error> {
  let mut opts = git2::StatusOptions::new();
  opts.include_untracked(true)
      .recurse_untracked_dirs(true)
      .include_ignored(false);

  let workdir = repo.workdir().ok_or_else(|| git2::Error::from_str("bare repo"))?;
  let statuses = repo.statuses(Some(&mut opts))?;

  for entry in statuses.iter() {
    let status = entry.status();
    if status.contains(Status::WT_NEW) {
      if let Some(path) = entry.path() {
        let abs = workdir.join(path);
        if abs.is_dir() {
          let _ = tokio::fs::remove_dir_all(&abs).await;
        } else {
          let _ = tokio::fs::remove_file(&abs).await;
        }
      }
    }
  }

  Ok(())
}

/// Forcibly updates the provided repository with the remote by fetching, then
/// performing a hard reset, and finally removes any untracked files or directories
/// recursively.
#[instrument(skip(repo))]
pub async fn update_repo(config: &MicropubContentGit, repo: &git2::Repository) -> Result<(), git2::Error> {
  fetch_repo(config, repo)?; // similar to `git fetch origin <branch>`
  reset_to_head(config, repo)?; // similar to `git reset --hard origin/<branch>`
  clean_repo(repo).await?; // similar to `git clean -fd`
  Ok(())
}

/// Adds all changes to the repository index (i.e., stages everything for commit)
#[instrument(skip(repo))]
pub fn add_all(repo: &git2::Repository) -> Result<git2::Oid, git2::Error> {
  let mut idx = repo.index()?;
  idx.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
  idx.write()?;
  idx.write_tree()
}

/// Commits a particular object ID (oid) to prepare for pushing. The Oid is
/// returned from `add_path`.
#[instrument(skip(repo))]
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

/// Adds all changes to the repository index, and then immediately commits them.
#[instrument(skip(repo))]
pub fn add_all_and_commit(repo: &git2::Repository, message: &str) -> Result<(), git2::Error> {
  let oid = add_all(repo)?;
  commit(repo, oid, message)?;
  Ok(())
} 

/// Pushes a branch of the repository to its remote equivalent.
#[instrument(skip(state, repo))]
pub fn push(state: &Arc<AppState>, repo: &git2::Repository, branch: &str) -> Result<(), git2::Error> {
  let mut remote = repo.find_remote("origin")?;

  let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

  let mut po = git2::PushOptions::new();
  po.remote_callbacks(get_remote_callbacks(&state.config.micropub.content.git));

  remote.push(&[&refspec], Some(&mut po))?;

  Ok(())
}
