use std::path::Path;
use unlinger_core::RuntimeArtifactCandidate;

#[test]
fn candidate_requires_an_absolute_profile_and_exact_filename() {
    let relative = Path::new("playwright_chromiumdev_profile-relative");
    assert!(RuntimeArtifactCandidate::devtools_active_port(relative, 501, "test-session").is_err());
}
