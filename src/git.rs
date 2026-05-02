use std::{error::Error, path::Path, sync::Arc};

use async_tempfile::TempDir;
use git2::{Cred, CredentialType, Remote, RemoteCallbacks};
use tracing::debug;

use crate::AppState;

pub fn get_remote_callbacks(state: &Arc<AppState>) -> RemoteCallbacks<'_> {
    let git = &state.config.micropub.storage.git;

    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(|_url, username_from_url, r#type| {
        debug!(
            "git credential callback invoked with credential type {:?}",
            r#type
        );

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

pub async fn clone_repo(state: &Arc<AppState>) -> Result<(git2::Repository, TempDir), Box<dyn Error>> {
    let location = TempDir::new().await?;

    debug!("created tempdir: {location:?}");

    let mut fo = git2::FetchOptions::new();
    fo.remote_callbacks(get_remote_callbacks(state));
    fo.depth(1);

    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fo);

    debug!("cloning git repository");
    Ok((builder.clone(&state.config.micropub.storage.git.repository, location.dir_path())?, location))
}

pub fn try_connect_repo(state: &Arc<AppState>) -> Result<(), git2::Error> {
    debug!("creating detached remote");
    let mut rem = Remote::create_detached(state.config.micropub.storage.git.repository.clone())?;
    debug!("connecting to remote");
    rem.connect_auth(
        git2::Direction::Fetch,
        Some(get_remote_callbacks(state)),
        None,
    )?;
    debug!("disconnecting from remote");
    rem.disconnect()?;

    debug!("git connection test ok");
    Ok(())
}
