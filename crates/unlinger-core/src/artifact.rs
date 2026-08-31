use crate::fingerprint_parts;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeArtifactKind {
    DevToolsActivePort,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactDisposition {
    Removed,
    AlreadyAbsent,
    IdentityMismatch,
    Referenced,
    Unsafe,
    Rejected,
    CancelledBeforeDelivery,
    /// Recovery projection for an unlink action left PREPARED across a crash.
    /// A live artifact adapter must never manufacture this disposition.
    DeliveryUnknown,
}

impl ArtifactDisposition {
    #[must_use]
    pub fn completed_cleanup(self) -> bool {
        matches!(self, Self::Removed | Self::AlreadyAbsent)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactActionIntent {
    pub kind: RuntimeArtifactKind,
    pub artifact_fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ArtifactAction {
    pub kind: RuntimeArtifactKind,
    pub artifact_fingerprint: String,
    pub disposition: ArtifactDisposition,
}

#[derive(Clone, Eq, PartialEq)]
pub struct RuntimeArtifactCandidate {
    kind: RuntimeArtifactKind,
    profile_path: PathBuf,
    artifact_path: PathBuf,
    owner_uid: u32,
    session_fingerprint: String,
    artifact_fingerprint: String,
}

impl RuntimeArtifactCandidate {
    pub fn devtools_active_port(
        profile_path: &Path,
        owner_uid: u32,
        session_fingerprint: &str,
    ) -> Result<Self, ArtifactCandidateError> {
        validate_profile_path(profile_path)?;
        if owner_uid == 0 {
            return Err(ArtifactCandidateError::RootOwner);
        }
        if session_fingerprint.trim().is_empty() {
            return Err(ArtifactCandidateError::MissingSessionFingerprint);
        }
        let profile_text = profile_path
            .to_str()
            .ok_or(ArtifactCandidateError::NonUtf8Path)?;
        let artifact_path = profile_path.join("DevToolsActivePort");
        let artifact_fingerprint = format!(
            "art-{}",
            fingerprint_parts([
                b"unlinger.runtime-artifact.v1".as_slice(),
                b"devtools_active_port".as_slice(),
                session_fingerprint.as_bytes(),
                profile_text.as_bytes(),
            ])
        );
        Ok(Self {
            kind: RuntimeArtifactKind::DevToolsActivePort,
            profile_path: profile_path.to_path_buf(),
            artifact_path,
            owner_uid,
            session_fingerprint: session_fingerprint.to_owned(),
            artifact_fingerprint,
        })
    }

    #[must_use]
    pub fn kind(&self) -> RuntimeArtifactKind {
        self.kind
    }

    /// Transient platform input. This path must never enter history, IPC, or diagnostics.
    #[must_use]
    pub fn profile_path(&self) -> &Path {
        &self.profile_path
    }

    /// Transient platform input. This path must never enter history, IPC, or diagnostics.
    #[must_use]
    pub fn artifact_path(&self) -> &Path {
        &self.artifact_path
    }

    #[must_use]
    pub fn owner_uid(&self) -> u32 {
        self.owner_uid
    }

    #[must_use]
    pub fn session_fingerprint(&self) -> &str {
        &self.session_fingerprint
    }

    #[must_use]
    pub fn artifact_fingerprint(&self) -> &str {
        &self.artifact_fingerprint
    }

    #[must_use]
    pub fn intent(&self) -> ArtifactActionIntent {
        ArtifactActionIntent {
            kind: self.kind,
            artifact_fingerprint: self.artifact_fingerprint.clone(),
        }
    }
}

impl Debug for RuntimeArtifactCandidate {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeArtifactCandidate")
            .field("kind", &self.kind)
            .field("owner_uid", &self.owner_uid)
            .field("artifact_fingerprint", &self.artifact_fingerprint)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeArtifactIdentity {
    pub device: u64,
    pub inode: u64,
    pub owner_uid: u32,
    pub mode: u32,
    pub link_count: u64,
    pub parent_device: u64,
    pub parent_inode: u64,
    pub parent_owner_uid: u32,
    pub parent_mode: u32,
}

#[derive(Clone, Eq, PartialEq)]
pub struct FrozenRuntimeArtifact {
    candidate: RuntimeArtifactCandidate,
    identity: RuntimeArtifactIdentity,
}

impl FrozenRuntimeArtifact {
    #[must_use]
    pub fn new(candidate: RuntimeArtifactCandidate, identity: RuntimeArtifactIdentity) -> Self {
        Self {
            candidate,
            identity,
        }
    }

    #[must_use]
    pub fn candidate(&self) -> &RuntimeArtifactCandidate {
        &self.candidate
    }

    #[must_use]
    pub fn identity(&self) -> &RuntimeArtifactIdentity {
        &self.identity
    }
}

impl Debug for FrozenRuntimeArtifact {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FrozenRuntimeArtifact")
            .field("candidate", &self.candidate)
            .field("identity", &self.identity)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ArtifactFreeze {
    Frozen(FrozenRuntimeArtifact),
    Absent,
    Unsafe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArtifactCandidateError {
    RelativePath,
    UnsafePathComponent,
    RootProfile,
    NonUtf8Path,
    RootOwner,
    MissingSessionFingerprint,
}

impl Display for ArtifactCandidateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::RelativePath => "runtime artifact profile path is not absolute",
            Self::UnsafePathComponent => {
                "runtime artifact profile path contains a non-normal component"
            }
            Self::RootProfile => "runtime artifact profile cannot be the filesystem root",
            Self::NonUtf8Path => "runtime artifact profile path is not valid UTF-8",
            Self::RootOwner => "runtime artifact cannot be owned by root",
            Self::MissingSessionFingerprint => "runtime artifact has no owning session fingerprint",
        };
        formatter.write_str(message)
    }
}

impl Error for ArtifactCandidateError {}

fn validate_profile_path(path: &Path) -> Result<(), ArtifactCandidateError> {
    if !path.is_absolute() {
        return Err(ArtifactCandidateError::RelativePath);
    }
    let mut normal_components = 0_usize;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(_) => normal_components += 1,
            Component::Prefix(_) | Component::CurDir | Component::ParentDir => {
                return Err(ArtifactCandidateError::UnsafePathComponent);
            }
        }
    }
    if normal_components == 0 {
        return Err(ArtifactCandidateError::RootProfile);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_action_projection_do_not_expose_the_raw_path() {
        let raw = "/private/tmp/playwright_chromiumdev_profile-private-name";
        let candidate =
            RuntimeArtifactCandidate::devtools_active_port(Path::new(raw), 501, "session-redacted")
                .expect("candidate");

        assert!(!format!("{candidate:?}").contains(raw));
        let json = serde_json::to_string(&ArtifactAction {
            kind: candidate.kind(),
            artifact_fingerprint: candidate.artifact_fingerprint().to_owned(),
            disposition: ArtifactDisposition::Removed,
        })
        .expect("action JSON");
        assert!(!json.contains(raw));
        assert!(json.contains("dev_tools_active_port"));
    }
}
