use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;
use unlinger_core::{
    BrowserCompatibility, BrowserCompatibilityDecision, BrowserProduct, CleanupPlan,
    EvidenceFamily, EvidenceItem, GateLedger, IncidentReport, IncidentRevalidator, IncidentState,
    ProcessGraph, ProcessRecord, ProcessRole, ProcessRoleCount, ProcessTarget, Revalidation,
    RevalidationPhase, RevalidationStatus, RootSummary, RuntimeArtifactCandidate, Snapshot,
    fingerprint_parts, fingerprint_process_identity,
};

const STANDARD_PROFILE_MARKERS: &[&str] = &[
    "/library/application support/google/chrome",
    "/library/application support/chromium",
    "/library/application support/microsoft edge",
];

const HEADLESS_MARKERS: &[&str] = &["--headless", "--headless=new", "--headless=old"];
const TRANSPORT_MARKERS: &[&str] = &["--remote-debugging-pipe", "--remote-debugging-port"];
const MINIMUM_CANDIDATE_AGE_MICROS: u64 = 60 * 1_000_000;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SignaturePack {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub supported_versions: String,
    pub controller_markers: Vec<String>,
    pub framework_markers: Vec<String>,
    pub ephemeral_profile_markers: Vec<String>,
    pub browser_executable_markers: Vec<String>,
    pub recorder_executable_basenames: Vec<String>,
    pub graceful_strategy: GracefulStrategy,
    pub version_policy: VersionPolicy,
    pub artifact_policy: ArtifactPolicy,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GracefulStrategy {
    OsTermOnly,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum VersionPolicy {
    Observational,
    ExactAllowlist {
        bundle_id: String,
        versions: Vec<String>,
    },
    BoundedRange {
        bundle_id: String,
        minimum_inclusive: String,
        maximum_inclusive: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactPolicy {
    pub devtools_active_port: bool,
}

impl SignaturePack {
    fn validate(&self) -> Result<(), RuleError> {
        if self.schema_version != 2 {
            return Err(RuleError::InvalidPack(format!(
                "{} has unsupported schema {}",
                self.id, self.schema_version
            )));
        }
        if self.id.trim().is_empty()
            || self.version.trim().is_empty()
            || self.supported_versions.trim().is_empty()
            || self.controller_markers.is_empty()
            || self.framework_markers.is_empty()
            || self.ephemeral_profile_markers.is_empty()
            || self.browser_executable_markers.is_empty()
            || self.recorder_executable_basenames.is_empty()
        {
            return Err(RuleError::InvalidPack(format!(
                "{} is missing a required field or marker family",
                self.id
            )));
        }
        for marker in self
            .controller_markers
            .iter()
            .chain(&self.framework_markers)
            .chain(&self.ephemeral_profile_markers)
            .chain(&self.browser_executable_markers)
        {
            if marker.trim().len() < 4 {
                return Err(RuleError::InvalidPack(format!(
                    "{} contains an unsafe short marker",
                    self.id
                )));
            }
        }
        let mut recorder_names = BTreeSet::new();
        for basename in &self.recorder_executable_basenames {
            let normalized = basename.trim().to_ascii_lowercase();
            if normalized.is_empty()
                || normalized != basename.as_str()
                || normalized.contains('/')
                || normalized.contains('\\')
                || !recorder_names.insert(normalized)
            {
                return Err(RuleError::InvalidPack(format!(
                    "{} contains an invalid or duplicate recorder basename",
                    self.id
                )));
            }
        }
        self.version_policy.validate(&self.id)?;
        Ok(())
    }
}

impl VersionPolicy {
    fn validate(&self, pack_id: &str) -> Result<(), RuleError> {
        match self {
            Self::Observational => Ok(()),
            Self::ExactAllowlist {
                bundle_id,
                versions,
            } => {
                validate_bundle_id(pack_id, bundle_id)?;
                if versions.is_empty() {
                    return Err(RuleError::InvalidPack(format!(
                        "{pack_id} exact version allowlist is empty"
                    )));
                }
                let mut unique = BTreeSet::new();
                for version in versions {
                    parse_dotted_version(version).ok_or_else(|| {
                        RuleError::InvalidPack(format!(
                            "{pack_id} contains an invalid exact version"
                        ))
                    })?;
                    if !unique.insert(version) {
                        return Err(RuleError::InvalidPack(format!(
                            "{pack_id} contains a duplicate exact version"
                        )));
                    }
                }
                Ok(())
            }
            Self::BoundedRange {
                bundle_id,
                minimum_inclusive,
                maximum_inclusive,
            } => {
                validate_bundle_id(pack_id, bundle_id)?;
                let minimum = parse_dotted_version(minimum_inclusive).ok_or_else(|| {
                    RuleError::InvalidPack(format!("{pack_id} contains an invalid minimum version"))
                })?;
                let maximum = parse_dotted_version(maximum_inclusive).ok_or_else(|| {
                    RuleError::InvalidPack(format!("{pack_id} contains an invalid maximum version"))
                })?;
                if minimum > maximum {
                    return Err(RuleError::InvalidPack(format!(
                        "{pack_id} version range is reversed"
                    )));
                }
                Ok(())
            }
        }
    }
}

fn validate_bundle_id(pack_id: &str, bundle_id: &str) -> Result<(), RuleError> {
    if bundle_id.is_empty()
        || bundle_id.len() > 192
        || bundle_id.trim() != bundle_id
        || !bundle_id.contains('.')
    {
        return Err(RuleError::InvalidPack(format!(
            "{pack_id} contains an invalid bundle id"
        )));
    }
    Ok(())
}

fn parse_dotted_version(value: &str) -> Option<Vec<u64>> {
    if value.is_empty() || value.len() > 64 {
        return None;
    }
    let components = value.split('.').collect::<Vec<_>>();
    if components.is_empty() || components.len() > 8 {
        return None;
    }
    let mut parsed = components
        .into_iter()
        .map(|component| {
            (!component.is_empty() && component.bytes().all(|byte| byte.is_ascii_digit()))
                .then(|| component.parse::<u64>().ok())
                .flatten()
        })
        .collect::<Option<Vec<_>>>()?;
    while parsed.len() > 1 && parsed.last() == Some(&0) {
        parsed.pop();
    }
    Some(parsed)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BrowserVersionGate {
    ExactSupported,
    RangeSupported,
    Observational,
    MissingFacts,
    MixedFacts,
    UnsupportedProduct,
    UnsupportedVersion,
}

impl BrowserVersionGate {
    fn supported(self) -> bool {
        matches!(self, Self::ExactSupported | Self::RangeSupported)
    }
}

fn evaluate_browser_versions(
    browser_roots: &[&ProcessRecord],
    policy: &VersionPolicy,
) -> BrowserVersionGate {
    let facts = browser_roots
        .iter()
        .filter_map(|process| process.runtime.app_bundle.as_ref())
        .collect::<Vec<_>>();
    if facts.len() != browser_roots.len() {
        return BrowserVersionGate::MissingFacts;
    }
    let unique = facts
        .iter()
        .map(|fact| (fact.bundle_id.as_str(), fact.short_version.as_str()))
        .collect::<BTreeSet<_>>();
    if unique.len() != 1 {
        return BrowserVersionGate::MixedFacts;
    }
    let fact = facts[0];
    match policy {
        VersionPolicy::Observational => BrowserVersionGate::Observational,
        VersionPolicy::ExactAllowlist {
            bundle_id,
            versions,
        } => {
            if fact.bundle_id != *bundle_id {
                BrowserVersionGate::UnsupportedProduct
            } else if versions.contains(&fact.short_version) {
                BrowserVersionGate::ExactSupported
            } else {
                BrowserVersionGate::UnsupportedVersion
            }
        }
        VersionPolicy::BoundedRange {
            bundle_id,
            minimum_inclusive,
            maximum_inclusive,
        } => {
            if fact.bundle_id != *bundle_id {
                return BrowserVersionGate::UnsupportedProduct;
            }
            let Some(version) = parse_dotted_version(&fact.short_version) else {
                return BrowserVersionGate::UnsupportedVersion;
            };
            let Some(minimum) = parse_dotted_version(minimum_inclusive) else {
                return BrowserVersionGate::UnsupportedVersion;
            };
            let Some(maximum) = parse_dotted_version(maximum_inclusive) else {
                return BrowserVersionGate::UnsupportedVersion;
            };
            if minimum <= version && version <= maximum {
                BrowserVersionGate::RangeSupported
            } else {
                BrowserVersionGate::UnsupportedVersion
            }
        }
    }
}

fn browser_product(bundle_id: &str) -> BrowserProduct {
    match bundle_id.to_ascii_lowercase().as_str() {
        "com.google.chrome.for.testing" => BrowserProduct::ChromeForTesting,
        "org.chromium.chromium" => BrowserProduct::Chromium,
        "com.google.chrome" => BrowserProduct::GoogleChrome,
        _ => BrowserProduct::Other,
    }
}

fn browser_compatibility(
    browser_roots: &[&ProcessRecord],
    version_gate: Option<BrowserVersionGate>,
    has_controller: bool,
    control_path_incomplete: bool,
) -> BrowserCompatibility {
    let facts = browser_roots
        .iter()
        .filter_map(|process| process.runtime.app_bundle.as_ref())
        .collect::<Vec<_>>();
    let bundle_ids = facts
        .iter()
        .map(|fact| fact.bundle_id.as_str())
        .collect::<BTreeSet<_>>();
    let versions = facts
        .iter()
        .map(|fact| fact.short_version.as_str())
        .collect::<BTreeSet<_>>();
    let product = if facts.len() == browser_roots.len() && bundle_ids.len() == 1 {
        browser_product(facts[0].bundle_id.as_str())
    } else {
        BrowserProduct::Unknown
    };
    let observed_version = (facts.len() == browser_roots.len() && versions.len() == 1)
        .then(|| facts[0].short_version.clone());

    let (decision, reason_id) = match version_gate {
        Some(BrowserVersionGate::MixedFacts) => (
            BrowserCompatibilityDecision::Protected,
            Some("protection.browser_version_mixed"),
        ),
        Some(BrowserVersionGate::UnsupportedProduct) => (
            BrowserCompatibilityDecision::Protected,
            Some("protection.browser_product_unsupported"),
        ),
        Some(BrowserVersionGate::UnsupportedVersion) => (
            BrowserCompatibilityDecision::Protected,
            Some("protection.browser_version_unsupported"),
        ),
        Some(BrowserVersionGate::MissingFacts) => (
            BrowserCompatibilityDecision::Unknown,
            Some("protection.browser_version_missing"),
        ),
        Some(BrowserVersionGate::Observational) => (
            BrowserCompatibilityDecision::ObserveOnly,
            Some("protection.version_observational_only"),
        ),
        Some(BrowserVersionGate::ExactSupported | BrowserVersionGate::RangeSupported)
            if has_controller =>
        {
            (
                BrowserCompatibilityDecision::Protected,
                Some("protection.controller_version_unverified"),
            )
        }
        Some(BrowserVersionGate::ExactSupported | BrowserVersionGate::RangeSupported)
            if control_path_incomplete =>
        {
            (
                BrowserCompatibilityDecision::Protected,
                Some("protection.debug_peer_visibility_incomplete"),
            )
        }
        Some(BrowserVersionGate::ExactSupported | BrowserVersionGate::RangeSupported) => {
            (BrowserCompatibilityDecision::Automatic, None)
        }
        None => (
            BrowserCompatibilityDecision::Unknown,
            Some("ambiguity.browser_root_missing"),
        ),
    };
    BrowserCompatibility {
        product,
        observed_version,
        decision,
        reason_id: reason_id.map(str::to_owned),
    }
}

fn policy_bundle_id(policy: &VersionPolicy) -> Option<&str> {
    match policy {
        VersionPolicy::Observational => None,
        VersionPolicy::ExactAllowlist { bundle_id, .. }
        | VersionPolicy::BoundedRange { bundle_id, .. } => Some(bundle_id),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CandidateAgeGate {
    Eligible,
    TooYoung,
    Unknown,
}

fn candidate_age_gate(
    observed_at_unix_millis: u64,
    started_at_unix_micros: u64,
) -> CandidateAgeGate {
    if started_at_unix_micros == 0 {
        return CandidateAgeGate::Unknown;
    }
    let Some(observed_at_unix_micros) = observed_at_unix_millis.checked_mul(1_000) else {
        return CandidateAgeGate::Unknown;
    };
    let Some(age) = observed_at_unix_micros.checked_sub(started_at_unix_micros) else {
        return CandidateAgeGate::Unknown;
    };
    if age < MINIMUM_CANDIDATE_AGE_MICROS {
        CandidateAgeGate::TooYoung
    } else {
        CandidateAgeGate::Eligible
    }
}

#[derive(Clone, Debug)]
pub struct RuleSet {
    packs: Vec<SignaturePack>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrowserAutomaticActionLevel {
    Automatic,
    ObserveOnly,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserFamilySupport {
    pub family: String,
    pub product: BrowserProduct,
    pub admitted_versions: Vec<String>,
    pub automatic_action_level: BrowserAutomaticActionLevel,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserSupportCatalog {
    pub support_revision: String,
    pub families: Vec<BrowserFamilySupport>,
}

impl RuleSet {
    pub fn embedded() -> Result<Self, RuleError> {
        let sources = [
            include_str!("../../../rules/agent-browser.toml"),
            include_str!("../../../rules/playwright.toml"),
            include_str!("../../../rules/puppeteer.toml"),
        ];
        let mut packs = Vec::with_capacity(sources.len());
        let mut ids = BTreeSet::new();
        for source in sources {
            let pack: SignaturePack =
                toml::from_str(source).map_err(|error| RuleError::Parse(error.to_string()))?;
            pack.validate()?;
            if !ids.insert(pack.id.clone()) {
                return Err(RuleError::InvalidPack(format!(
                    "duplicate pack id {}",
                    pack.id
                )));
            }
            packs.push(pack);
        }
        packs.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(Self { packs })
    }

    #[must_use]
    pub fn packs(&self) -> &[SignaturePack] {
        &self.packs
    }

    #[must_use]
    pub fn browser_support_catalog(&self) -> BrowserSupportCatalog {
        let support_revision = format!(
            "rules:{}",
            self.packs
                .iter()
                .map(|pack| format!("{}@{}", pack.id, pack.version))
                .collect::<Vec<_>>()
                .join("|")
        );
        let families = self
            .packs
            .iter()
            .map(|pack| {
                let (product, admitted_versions, automatic_action_level) =
                    match &pack.version_policy {
                        VersionPolicy::Observational => (
                            BrowserProduct::Unknown,
                            Vec::new(),
                            BrowserAutomaticActionLevel::ObserveOnly,
                        ),
                        VersionPolicy::ExactAllowlist {
                            bundle_id,
                            versions,
                        } => (
                            browser_product(bundle_id),
                            versions.clone(),
                            BrowserAutomaticActionLevel::Automatic,
                        ),
                        VersionPolicy::BoundedRange {
                            bundle_id,
                            minimum_inclusive,
                            maximum_inclusive,
                        } => (
                            browser_product(bundle_id),
                            vec![format!("{minimum_inclusive}..={maximum_inclusive}")],
                            BrowserAutomaticActionLevel::Automatic,
                        ),
                    };
                BrowserFamilySupport {
                    family: pack.id.clone(),
                    product,
                    admitted_versions,
                    automatic_action_level,
                }
            })
            .collect();
        BrowserSupportCatalog {
            support_revision,
            families,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RuleError {
    Parse(String),
    InvalidPack(String),
    InvalidGraph(String),
}

impl Display for RuleError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(message) => write!(formatter, "could not parse signature pack: {message}"),
            Self::InvalidPack(message) => write!(formatter, "invalid signature pack: {message}"),
            Self::InvalidGraph(message) => write!(formatter, "invalid process graph: {message}"),
        }
    }
}

impl Error for RuleError {}

#[derive(Clone, Debug, Default)]
pub struct AnalyzerContext {
    pub self_pid: Option<u32>,
    pub ancestor_pids: BTreeSet<u32>,
}

#[derive(Clone, Debug)]
pub struct Analyzer {
    rules: RuleSet,
    context: AnalyzerContext,
}

#[derive(Clone, Debug)]
struct Candidate<'a> {
    pack: &'a SignaturePack,
    root_pid: u32,
    controller_pid: Option<u32>,
    browser_root_pids: Vec<u32>,
    member_pids: Vec<u32>,
    pack_anchor_count: usize,
}

impl Analyzer {
    #[must_use]
    pub fn new(rules: RuleSet, context: AnalyzerContext) -> Self {
        Self { rules, context }
    }

    #[must_use]
    pub fn rules(&self) -> &RuleSet {
        &self.rules
    }

    pub fn observe(&self, snapshot: &Snapshot) -> Result<Vec<IncidentReport>, RuleError> {
        let graph = ProcessGraph::from_snapshot(snapshot)
            .map_err(|error| RuleError::InvalidGraph(error.to_string()))?;
        let candidates = self.candidates(&graph);
        let mut reports = candidates
            .iter()
            .filter_map(|candidate| {
                self.classify(&graph, candidate, snapshot.observed_at_unix_millis)
            })
            .collect::<Vec<_>>();
        reports.sort_by(|left, right| left.incident_id.cmp(&right.incident_id));
        Ok(reports)
    }

    #[must_use]
    pub fn reconcile(
        &self,
        first: &[IncidentReport],
        second: &[IncidentReport],
    ) -> Vec<IncidentReport> {
        self.reconcile_with_abandonment(first, second, &BTreeSet::new())
    }

    #[must_use]
    pub fn reconcile_with_abandonment(
        &self,
        first: &[IncidentReport],
        second: &[IncidentReport],
        confirmed_tracking_keys: &BTreeSet<String>,
    ) -> Vec<IncidentReport> {
        let first_by_key = first
            .iter()
            .map(|report| (report.tracking_key.as_str(), report))
            .collect::<BTreeMap<_, _>>();
        let mut reconciled = second.to_vec();
        for report in &mut reconciled {
            if report.state != IncidentState::Cooling {
                continue;
            }
            let Some(previous) = first_by_key.get(report.tracking_key.as_str()) else {
                continue;
            };
            if previous.state != IncidentState::Cooling {
                continue;
            }
            let unchanged = previous.root.identity_fingerprint == report.root.identity_fingerprint
                && previous.member_fingerprint == report.member_fingerprint;
            report.gates.stable_across_two_observations = unchanged;
            report.gates.process_identity_unchanged = unchanged;
            report.gates.confirmed_abandonment =
                confirmed_tracking_keys.contains(&report.tracking_key);
            if unchanged && report.gates.cleanup_eligible() {
                report.state = IncidentState::Confirmed;
                report.evidence.push(EvidenceItem {
                    id: "abandonment.grace_elapsed".to_owned(),
                    family: EvidenceFamily::Abandonment,
                    source_pid: Some(report.root.pid),
                });
                report.evidence.push(EvidenceItem {
                    id: "observation.stable_twice".to_owned(),
                    family: EvidenceFamily::Abandonment,
                    source_pid: Some(report.root.pid),
                });
            } else if !unchanged {
                report.state = IncidentState::Ambiguous;
                report.gates.no_protection_rule = false;
                report.evidence.push(EvidenceItem {
                    id: "protection.identity_or_membership_changed".to_owned(),
                    family: EvidenceFamily::Protection,
                    source_pid: Some(report.root.pid),
                });
            }
            sort_evidence(&mut report.evidence);
        }
        reconciled
    }

    fn candidates<'a>(&'a self, graph: &ProcessGraph) -> Vec<Candidate<'a>> {
        let mut selected: BTreeMap<u32, Candidate<'a>> = BTreeMap::new();
        for pack in self.rules.packs() {
            let browser_roots = graph
                .processes()
                .filter(|process| is_browser_root(process, pack))
                .map(ProcessRecord::pid)
                .collect::<Vec<_>>();
            let browser_root_set = browser_roots.iter().copied().collect::<BTreeSet<_>>();
            let controllers = graph
                .processes()
                .filter(|process| {
                    !browser_root_set.contains(&process.pid()) && matches_controller(process, pack)
                })
                .map(ProcessRecord::pid)
                .collect::<BTreeSet<_>>();

            let mut by_controller: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
            let mut unowned = Vec::new();
            for browser_pid in browser_roots {
                let nearest_controller = graph
                    .ancestor_pids(browser_pid)
                    .into_iter()
                    .find(|pid| controllers.contains(pid));
                if let Some(controller_pid) = nearest_controller {
                    by_controller
                        .entry(controller_pid)
                        .or_default()
                        .push(browser_pid);
                } else if graph
                    .get(browser_pid)
                    .is_some_and(|process| pack_anchor_count(process, pack) > 0)
                {
                    unowned.push(browser_pid);
                }
            }

            let controllers_with_browsers = by_controller.keys().copied().collect::<BTreeSet<_>>();
            for (controller_pid, mut browsers) in by_controller {
                browsers.sort_unstable();
                let mut members = vec![controller_pid];
                for browser_pid in &browsers {
                    members.extend(graph.descendant_pids(*browser_pid));
                }
                members.extend(joined_recorder_pids(graph, controller_pid, &browsers, pack));
                members.sort_unstable();
                members.dedup();
                let candidate = Candidate {
                    pack,
                    root_pid: controller_pid,
                    controller_pid: Some(controller_pid),
                    browser_root_pids: browsers,
                    member_pids: members,
                    pack_anchor_count: graph
                        .get(controller_pid)
                        .map_or(0, |process| pack_anchor_count(process, pack)),
                };
                select_candidate(&mut selected, candidate);
            }

            for controller_pid in controllers.difference(&controllers_with_browsers) {
                let Some(process) = graph.get(*controller_pid) else {
                    continue;
                };
                let candidate = Candidate {
                    pack,
                    root_pid: *controller_pid,
                    controller_pid: Some(*controller_pid),
                    browser_root_pids: Vec::new(),
                    member_pids: vec![*controller_pid],
                    pack_anchor_count: pack_anchor_count(process, pack),
                };
                select_candidate(&mut selected, candidate);
            }

            for browser_pid in unowned {
                let Some(process) = graph.get(browser_pid) else {
                    continue;
                };
                let candidate = Candidate {
                    pack,
                    root_pid: browser_pid,
                    controller_pid: None,
                    browser_root_pids: vec![browser_pid],
                    member_pids: graph.descendant_pids(browser_pid),
                    pack_anchor_count: pack_anchor_count(process, pack),
                };
                select_candidate(&mut selected, candidate);
            }
        }
        selected.into_values().collect()
    }

    fn classify(
        &self,
        graph: &ProcessGraph,
        candidate: &Candidate<'_>,
        observed_at_unix_millis: u64,
    ) -> Option<IncidentReport> {
        let root = graph.get(candidate.root_pid)?;
        let members = candidate
            .member_pids
            .iter()
            .filter_map(|pid| graph.get(*pid))
            .collect::<Vec<_>>();
        let browser_roots = candidate
            .browser_root_pids
            .iter()
            .filter_map(|pid| graph.get(*pid))
            .collect::<Vec<_>>();
        let has_browser_roots = !browser_roots.is_empty();

        let mut evidence = Vec::new();
        let mut protection = BTreeSet::new();
        let same_user = members
            .iter()
            .all(|process| process.uid == graph.current_uid() && process.uid != 0);
        if !same_user {
            protection.insert("protection.uid_or_root".to_owned());
        }

        if candidate.member_pids.contains(&0) || candidate.member_pids.contains(&1) {
            protection.insert("protection.system_pid".to_owned());
        }
        if self
            .context
            .self_pid
            .is_some_and(|pid| candidate.member_pids.contains(&pid))
            || candidate
                .member_pids
                .iter()
                .any(|pid| self.context.ancestor_pids.contains(pid))
        {
            protection.insert("protection.unlinger_or_ancestor".to_owned());
        }

        if candidate
            .member_pids
            .iter()
            .any(|pid| graph.lineage_has_cycle(*pid) || graph.has_unresolved_parent(*pid))
        {
            protection.insert("protection.incomplete_or_cyclic_graph".to_owned());
        }
        if members
            .iter()
            .any(|process| !process.has_complete_classification_facts())
        {
            protection.insert("protection.incomplete_process_facts".to_owned());
        }

        let has_controller = candidate.controller_pid.is_some();
        let framework_argument = candidate
            .controller_pid
            .and_then(|pid| graph.get(pid))
            .into_iter()
            .chain(browser_roots.iter().copied())
            .any(|process| process_contains_any(process, &candidate.pack.framework_markers));
        let headless = has_browser_roots
            && browser_roots
                .iter()
                .all(|process| args_contain_any(process, HEADLESS_MARKERS));
        let transport = browser_roots
            .iter()
            .any(|process| args_contain_any(process, TRANSPORT_MARKERS));
        let debug_pipe = browser_roots
            .iter()
            .any(|process| args_contain(process, "--remote-debugging-pipe"));
        let version_gate = has_browser_roots
            .then(|| evaluate_browser_versions(&browser_roots, &candidate.pack.version_policy));
        let recognized_browser_product = has_browser_roots
            && policy_bundle_id(&candidate.pack.version_policy).is_some_and(|bundle_id| {
                browser_roots.iter().all(|process| {
                    process
                        .runtime
                        .app_bundle
                        .as_ref()
                        .is_some_and(|fact| fact.bundle_id == bundle_id)
                })
            });

        let profile_paths = browser_roots
            .iter()
            .filter_map(|process| flag_value(process, "--user-data-dir"))
            .collect::<Vec<_>>();
        let standard_profile = profile_paths
            .iter()
            .any(|path| contains_any(path, STANDARD_PROFILE_MARKERS));
        let ephemeral_profile = !profile_paths.is_empty()
            && profile_paths.iter().all(|path| {
                contains_any(path, &candidate.pack.ephemeral_profile_markers)
                    && is_ephemeral_path(path)
            });
        let persistent_profile = !profile_paths.is_empty() && !ephemeral_profile;
        let manual_cdp = !has_controller
            && !ephemeral_profile
            && browser_roots
                .iter()
                .any(|process| args_contain(process, "--remote-debugging-port"));
        let shared_profile = profile_paths.iter().any(|profile| {
            graph.processes().any(|process| {
                !candidate.member_pids.contains(&process.pid())
                    && flag_value(process, "--user-data-dir")
                        .is_some_and(|other| normalize(&other) == normalize(profile))
            })
        });

        if standard_profile {
            protection.insert("protection.standard_browser_profile".to_owned());
        }
        if has_browser_roots && !headless {
            protection.insert("protection.headed_or_human_control".to_owned());
        }
        if persistent_profile {
            protection.insert("protection.persistent_or_unknown_profile".to_owned());
        }
        if manual_cdp {
            protection.insert("protection.manual_cdp".to_owned());
        }
        if shared_profile {
            protection.insert("protection.profile_shared_outside_incident".to_owned());
        }
        let has_debug_port = browser_roots
            .iter()
            .any(|process| args_contain(process, "--remote-debugging-port"));
        if browser_roots
            .iter()
            .any(|process| process.runtime.attached_debug_transport)
        {
            protection.insert("protection.attached_debug_peer".to_owned());
        }
        let control_path_incomplete = has_debug_port
            && browser_roots
                .iter()
                .any(|process| !process.runtime.debug_transport_facts_complete);
        if control_path_incomplete {
            protection.insert("protection.debug_peer_visibility_incomplete".to_owned());
        }
        if has_controller {
            protection.insert("protection.controller_version_unverified".to_owned());
        }
        match version_gate {
            Some(BrowserVersionGate::ExactSupported | BrowserVersionGate::RangeSupported) => {}
            Some(BrowserVersionGate::Observational) => {
                protection.insert("protection.version_observational_only".to_owned());
            }
            Some(BrowserVersionGate::MissingFacts) => {
                protection.insert("protection.browser_version_missing".to_owned());
            }
            Some(BrowserVersionGate::MixedFacts) => {
                protection.insert("protection.browser_version_mixed".to_owned());
            }
            Some(BrowserVersionGate::UnsupportedProduct) => {
                protection.insert("protection.browser_product_unsupported".to_owned());
            }
            Some(BrowserVersionGate::UnsupportedVersion) => {
                protection.insert("protection.browser_version_unsupported".to_owned());
            }
            None => evidence.push(item(
                "ambiguity.browser_root_missing",
                EvidenceFamily::Abandonment,
                candidate.controller_pid,
            )),
        }
        match candidate_age_gate(
            observed_at_unix_millis,
            root.identity.started_at_unix_micros,
        ) {
            CandidateAgeGate::Eligible => {}
            CandidateAgeGate::TooYoung => {
                protection.insert("protection.minimum_candidate_age_not_met".to_owned());
            }
            CandidateAgeGate::Unknown => {
                protection.insert("protection.candidate_age_unknown".to_owned());
            }
        }
        if has_browser_roots && has_plausible_unjoined_recorder(graph, candidate, &browser_roots) {
            protection.insert("protection.unjoined_recorder_residue".to_owned());
        }

        let mut provenance_categories = 0usize;
        if has_controller {
            provenance_categories += 1;
            evidence.push(item(
                "provenance.controller_lineage",
                EvidenceFamily::AutomationProvenance,
                candidate.controller_pid,
            ));
        }
        if framework_argument {
            provenance_categories += 1;
            evidence.push(item(
                "provenance.framework_marker",
                EvidenceFamily::AutomationProvenance,
                Some(root.pid()),
            ));
        }
        if ephemeral_profile {
            provenance_categories += 1;
            evidence.push(item(
                "provenance.ephemeral_profile",
                EvidenceFamily::AutomationProvenance,
                candidate.browser_root_pids.first().copied(),
            ));
            evidence.push(item(
                "isolation.dedicated_ephemeral_profile",
                EvidenceFamily::Isolation,
                candidate.browser_root_pids.first().copied(),
            ));
        }
        if transport {
            provenance_categories += 1;
            evidence.push(item(
                if debug_pipe {
                    "provenance.private_debug_pipe"
                } else {
                    "provenance.debug_port"
                },
                EvidenceFamily::AutomationProvenance,
                candidate.browser_root_pids.first().copied(),
            ));
        }
        if headless {
            provenance_categories += 1;
            evidence.push(item(
                "provenance.headless_mode",
                EvidenceFamily::AutomationProvenance,
                candidate.browser_root_pids.first().copied(),
            ));
        }
        if recognized_browser_product {
            provenance_categories += 1;
            evidence.push(item(
                "provenance.automation_browser_binary",
                EvidenceFamily::AutomationProvenance,
                candidate.browser_root_pids.first().copied(),
            ));
        }
        match version_gate {
            Some(BrowserVersionGate::ExactSupported) => evidence.push(item(
                "version.browser_exact_allowlist",
                EvidenceFamily::AutomationProvenance,
                candidate.browser_root_pids.first().copied(),
            )),
            Some(BrowserVersionGate::RangeSupported) => evidence.push(item(
                "version.browser_bounded_range",
                EvidenceFamily::AutomationProvenance,
                candidate.browser_root_pids.first().copied(),
            )),
            _ => {}
        }
        let framework_anchor = has_controller || framework_argument || ephemeral_profile;
        let strong_provenance = framework_anchor && provenance_categories >= 3;

        let controller_parent_live = candidate.controller_pid.is_some_and(|pid| {
            graph.get(pid).is_some_and(|controller| {
                controller.parent_pid > 1 && graph.get(controller.parent_pid).is_some()
            })
        });
        let controller_reparented = candidate.controller_pid.is_some_and(|pid| {
            graph
                .get(pid)
                .is_some_and(|controller| controller.parent_pid == 1)
        });
        let owner_missing = controller_reparented
            || (candidate.controller_pid.is_none()
                && has_browser_roots
                && browser_roots.iter().all(|browser| {
                    browser.parent_pid == 1 || graph.get(browser.parent_pid).is_none()
                }));
        if controller_parent_live {
            evidence.push(item(
                "abandonment.live_controller_owner",
                EvidenceFamily::Abandonment,
                candidate.controller_pid,
            ));
        } else if controller_reparented {
            evidence.push(item(
                "abandonment.controller_reparented",
                EvidenceFamily::Abandonment,
                candidate.controller_pid,
            ));
        } else if owner_missing {
            evidence.push(item(
                "abandonment.controller_missing",
                EvidenceFamily::Abandonment,
                Some(root.pid()),
            ));
        }

        let isolated = ephemeral_profile && !shared_profile && !standard_profile;
        for id in &protection {
            evidence.push(item(id, EvidenceFamily::Protection, Some(root.pid())));
        }
        sort_evidence(&mut evidence);

        let gates = GateLedger {
            same_user,
            strong_automation_provenance: strong_provenance,
            confirmed_abandonment: false,
            isolated_session: isolated,
            stable_across_two_observations: false,
            process_identity_unchanged: false,
            no_protection_rule: protection.is_empty(),
        };
        let state = if !protection.is_empty() {
            IncidentState::Protected
        } else if controller_parent_live {
            IncidentState::Active
        } else if strong_provenance && owner_missing && isolated {
            IncidentState::Cooling
        } else {
            IncidentState::Ambiguous
        };

        let identity_fingerprint = fingerprint_process_identity(&root.identity);
        let incident_key = format!(
            "{}:{}:{}",
            candidate.pack.id, root.identity.pid, identity_fingerprint
        );
        let tracking_key_row = format!("{}:{}", candidate.pack.id, root.identity.pid);
        let mut role_counts = BTreeMap::<ProcessRole, usize>::new();
        let mut targets = Vec::with_capacity(members.len());
        for process in &members {
            let role = process_role(process, candidate);
            *role_counts.entry(role).or_default() += 1;
            targets.push(ProcessTarget {
                identity: process.identity.clone(),
                process_group_id: process.process_group_id,
                role,
            });
        }
        let mut session_rows = profile_paths
            .iter()
            .map(|path| normalize(path))
            .collect::<Vec<_>>();
        session_rows.sort_unstable();
        let session_fingerprint = if session_rows.is_empty() {
            let fallback = format!("{}:{}", candidate.pack.id, identity_fingerprint);
            format!("ses-{}", fingerprint_parts([fallback.as_bytes()]))
        } else {
            format!(
                "ses-{}",
                fingerprint_parts(session_rows.iter().map(String::as_bytes))
            )
        };
        let mut unique_profiles = BTreeMap::new();
        for profile in &profile_paths {
            unique_profiles
                .entry(normalize(profile))
                .or_insert_with(|| profile.clone());
        }
        let runtime_artifacts = if candidate.pack.artifact_policy.devtools_active_port
            && version_gate.is_some_and(BrowserVersionGate::supported)
            && protection.is_empty()
            && strong_provenance
            && owner_missing
            && ephemeral_profile
            && !shared_profile
            && !standard_profile
            && unique_profiles.len() == 1
        {
            unique_profiles
                .into_values()
                .next()
                .and_then(|profile| {
                    RuntimeArtifactCandidate::devtools_active_port(
                        Path::new(&profile),
                        graph.current_uid(),
                        &session_fingerprint,
                    )
                    .ok()
                })
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        let browser_compatibility = browser_compatibility(
            &browser_roots,
            version_gate,
            has_controller,
            control_path_incomplete,
        );

        Some(IncidentReport {
            incident_id: format!("inc-{}", fingerprint_parts([incident_key.as_bytes()])),
            tracking_key: format!("trk-{}", fingerprint_parts([tracking_key_row.as_bytes()])),
            session_fingerprint,
            signature_pack: candidate.pack.id.clone(),
            signature_version: candidate.pack.version.clone(),
            state,
            root: RootSummary {
                pid: root.pid(),
                started_at_unix_micros: root.identity.started_at_unix_micros,
                executable_basename: root.executable_basename(),
                identity_fingerprint,
            },
            member_count: members.len(),
            resident_memory_bytes: members
                .iter()
                .map(|process| process.resident_memory_bytes)
                .sum(),
            member_fingerprint: graph.process_set_fingerprint(&candidate.member_pids),
            roles: role_counts
                .into_iter()
                .map(|(role, count)| ProcessRoleCount { role, count })
                .collect(),
            evidence,
            gates,
            browser_compatibility,
            targets,
            runtime_artifacts,
        })
    }
}

impl IncidentRevalidator for Analyzer {
    fn revalidate(
        &self,
        snapshot: &Snapshot,
        plan: &CleanupPlan,
        phase: RevalidationPhase,
    ) -> Revalidation {
        if revalidation_facts_incomplete(self, snapshot, plan) {
            return Revalidation {
                status: RevalidationStatus::Blocked,
                reason_id: "revalidation.relevant_facts_incomplete".to_owned(),
            };
        }
        let reports = match self.observe(snapshot) {
            Ok(reports) => reports,
            Err(_) => {
                return Revalidation {
                    status: RevalidationStatus::Blocked,
                    reason_id: "revalidation.classifier_failed".to_owned(),
                };
            }
        };
        let matching = reports
            .iter()
            .filter(|report| report.session_fingerprint == plan.session_fingerprint)
            .collect::<Vec<_>>();

        if phase == RevalidationPhase::RevivalCheck {
            return if matching.is_empty() {
                Revalidation {
                    status: RevalidationStatus::Gone,
                    reason_id: "revalidation.session_absent".to_owned(),
                }
            } else {
                Revalidation {
                    status: RevalidationStatus::Revived,
                    reason_id: "revalidation.session_reappeared".to_owned(),
                }
            };
        }

        let plan_identities = plan
            .targets
            .iter()
            .map(|target| (target.identity.pid, &target.identity))
            .collect::<BTreeMap<_, _>>();
        let current_targets = snapshot
            .processes
            .iter()
            .filter(|process| plan_identities.contains_key(&process.pid()))
            .collect::<Vec<_>>();
        if current_targets.is_empty() {
            return Revalidation {
                status: RevalidationStatus::Gone,
                reason_id: "revalidation.frozen_targets_absent".to_owned(),
            };
        }
        if current_targets.iter().any(|process| {
            plan_identities
                .get(&process.pid())
                .is_none_or(|identity| !identity.exact_match(&process.identity))
        }) {
            return Revalidation {
                status: RevalidationStatus::Blocked,
                reason_id: "revalidation.identity_changed".to_owned(),
            };
        }
        if current_targets.iter().any(|process| {
            process.uid == 0
                || process.uid != snapshot.current_uid
                || !process.has_complete_classification_facts()
                || self.context.self_pid == Some(process.pid())
                || self.context.ancestor_pids.contains(&process.pid())
        }) {
            return Revalidation {
                status: RevalidationStatus::Blocked,
                reason_id: "revalidation.current_target_protected".to_owned(),
            };
        }
        if reports.iter().any(|report| {
            report.session_fingerprint != plan.session_fingerprint
                && report.targets.iter().any(|target| {
                    plan_identities
                        .get(&target.identity.pid)
                        .is_some_and(|identity| identity.exact_match(&target.identity))
                })
        }) {
            return Revalidation {
                status: RevalidationStatus::Blocked,
                reason_id: "revalidation.overlapping_session_changed".to_owned(),
            };
        }
        if matching.iter().any(|report| {
            report.state != IncidentState::Cooling
                || !report.gates.same_user
                || !report.gates.strong_automation_provenance
                || !report.gates.isolated_session
                || !report.gates.no_protection_rule
        }) {
            return Revalidation {
                status: RevalidationStatus::Blocked,
                reason_id: "revalidation.confidence_downgraded".to_owned(),
            };
        }
        if matching.iter().any(|report| {
            report.targets.iter().any(|target| {
                plan_identities
                    .get(&target.identity.pid)
                    .is_none_or(|identity| !identity.exact_match(&target.identity))
            })
        }) {
            return Revalidation {
                status: RevalidationStatus::Blocked,
                reason_id: "revalidation.new_or_changed_member".to_owned(),
            };
        }

        Revalidation {
            status: RevalidationStatus::Eligible,
            reason_id: if matching.is_empty() {
                "revalidation.exact_frozen_subset".to_owned()
            } else {
                "revalidation.incident_still_eligible".to_owned()
            },
        }
    }
}

fn revalidation_facts_incomplete(
    analyzer: &Analyzer,
    snapshot: &Snapshot,
    plan: &CleanupPlan,
) -> bool {
    if snapshot.coverage.unreadable_processes != 0
        || snapshot.coverage.listed_processes
            != snapshot
                .coverage
                .inspected_processes
                .saturating_add(snapshot.coverage.unreadable_processes)
        || snapshot.processes.len() != snapshot.coverage.inspected_processes
    {
        return true;
    }
    let Some(pack) = analyzer
        .rules
        .packs()
        .iter()
        .find(|pack| pack.id == plan.signature_pack && pack.version == plan.signature_version)
    else {
        return true;
    };
    let target_pids = plan
        .targets
        .iter()
        .map(|target| target.identity.pid)
        .collect::<BTreeSet<_>>();
    let target_process_groups = plan
        .targets
        .iter()
        .map(|target| target.process_group_id)
        .filter(|process_group_id| *process_group_id != 0)
        .collect::<BTreeSet<_>>();
    let target_executables = plan
        .targets
        .iter()
        .filter_map(|target| {
            Some((
                target.identity.executable_device?,
                target.identity.executable_inode?,
            ))
        })
        .collect::<BTreeSet<_>>();

    snapshot.processes.iter().any(|process| {
        if process.has_complete_classification_facts() {
            return false;
        }
        let executable_matches = process
            .identity
            .executable_device
            .zip(process.identity.executable_inode)
            .is_some_and(|identity| target_executables.contains(&identity));
        target_pids.contains(&process.pid())
            || (process.process_group_id != 0
                && target_process_groups.contains(&process.process_group_id))
            || executable_matches
            || matches_controller(process, pack)
            || is_browser_root(process, pack)
    })
}

fn select_candidate<'a>(selected: &mut BTreeMap<u32, Candidate<'a>>, candidate: Candidate<'a>) {
    let replace = selected.get(&candidate.root_pid).is_none_or(|existing| {
        candidate.pack_anchor_count > existing.pack_anchor_count
            || (candidate.pack_anchor_count == existing.pack_anchor_count
                && candidate.pack.id < existing.pack.id)
    });
    if replace {
        selected.insert(candidate.root_pid, candidate);
    }
}

fn joined_recorder_pids(
    graph: &ProcessGraph,
    controller_pid: u32,
    browser_root_pids: &[u32],
    pack: &SignaturePack,
) -> Vec<u32> {
    let Some(controller) = graph.get(controller_pid) else {
        return Vec::new();
    };
    if controller.process_group_id == 0
        || !browser_root_pids.iter().any(|pid| {
            graph.get(*pid).is_some_and(|browser| {
                browser.process_group_id != 0
                    && browser.process_group_id == controller.process_group_id
            })
        })
    {
        return Vec::new();
    }
    graph
        .processes()
        .filter(|process| {
            process.parent_pid == controller_pid
                && process.uid == controller.uid
                && process.tty_device.is_none()
                && process.identity.started_at_unix_micros
                    >= controller.identity.started_at_unix_micros
                && process.process_group_id == controller.process_group_id
                && matches_recorder(process, pack)
        })
        .map(ProcessRecord::pid)
        .collect()
}

fn has_plausible_unjoined_recorder(
    graph: &ProcessGraph,
    candidate: &Candidate<'_>,
    browser_roots: &[&ProcessRecord],
) -> bool {
    graph.processes().any(|process| {
        !candidate.member_pids.contains(&process.pid())
            && matches_recorder(process, candidate.pack)
            && process.uid == graph.current_uid()
            && process.tty_device.is_none()
            && browser_roots.iter().any(|browser| {
                process.identity.started_at_unix_micros >= browser.identity.started_at_unix_micros
                    && process.process_group_id != 0
                    && process.process_group_id == browser.process_group_id
            })
    })
}

fn matches_controller(process: &ProcessRecord, pack: &SignaturePack) -> bool {
    let basename = process.executable_basename().to_ascii_lowercase();
    if matches_recorder(process, pack)
        || basename.contains("crashpad")
        || process.arguments.as_ref().is_some_and(|arguments| {
            arguments
                .iter()
                .any(|argument| argument.to_ascii_lowercase().starts_with("--type="))
        })
    {
        return false;
    }
    let executable = process
        .executable_path
        .as_deref()
        .unwrap_or(&process.name)
        .to_ascii_lowercase();
    if contains_any(&executable, &pack.controller_markers) {
        return true;
    }
    process.arguments.as_ref().is_some_and(|arguments| {
        arguments
            .iter()
            .filter(|argument| !argument.starts_with('-'))
            .any(|argument| contains_any(argument, &pack.controller_markers))
    })
}

fn matches_recorder(process: &ProcessRecord, pack: &SignaturePack) -> bool {
    let basename = process.executable_basename().to_ascii_lowercase();
    pack.recorder_executable_basenames.contains(&basename)
}

fn is_browser_root(process: &ProcessRecord, pack: &SignaturePack) -> bool {
    let executable = process.executable_basename().to_ascii_lowercase();
    contains_any(&executable, &pack.browser_executable_markers)
        && !process.arguments.as_ref().is_some_and(|arguments| {
            arguments
                .iter()
                .any(|argument| argument.to_ascii_lowercase().starts_with("--type="))
        })
}

fn pack_anchor_count(process: &ProcessRecord, pack: &SignaturePack) -> usize {
    usize::from(process_contains_any(process, &pack.framework_markers))
        + usize::from(
            flag_value(process, "--user-data-dir")
                .is_some_and(|path| contains_any(&path, &pack.ephemeral_profile_markers)),
        )
}

fn process_contains_any(process: &ProcessRecord, markers: &[String]) -> bool {
    let mut parts = vec![process.name.as_str()];
    if let Some(path) = &process.executable_path {
        parts.push(path);
    }
    if let Some(arguments) = &process.arguments {
        parts.extend(arguments.iter().map(String::as_str));
    }
    parts.into_iter().any(|part| contains_any(part, markers))
}

fn args_contain(process: &ProcessRecord, marker: &str) -> bool {
    process.arguments.as_ref().is_some_and(|arguments| {
        arguments.iter().any(|argument| {
            let argument = argument.to_ascii_lowercase();
            let marker = marker.to_ascii_lowercase();
            argument == marker || argument.starts_with(&format!("{marker}="))
        })
    })
}

fn args_contain_any(process: &ProcessRecord, markers: &[&str]) -> bool {
    markers.iter().any(|marker| args_contain(process, marker))
}

fn flag_value(process: &ProcessRecord, flag: &str) -> Option<String> {
    let arguments = process.arguments.as_ref()?;
    let normalized_flag = flag.to_ascii_lowercase();
    for (index, argument) in arguments.iter().enumerate() {
        let normalized = argument.to_ascii_lowercase();
        if normalized == normalized_flag {
            return arguments.get(index + 1).cloned();
        }
        if let Some(value) = normalized.strip_prefix(&format!("{normalized_flag}=")) {
            let offset = argument.len().saturating_sub(value.len());
            return Some(argument[offset..].to_owned());
        }
    }
    None
}

fn contains_any<T: AsRef<str>>(value: &str, markers: &[T]) -> bool {
    let value = normalize(value);
    markers
        .iter()
        .any(|marker| value.contains(&normalize(marker.as_ref())))
}

fn normalize(value: &str) -> String {
    value.to_ascii_lowercase().replace('\\', "/")
}

fn is_ephemeral_path(path: &str) -> bool {
    let path = normalize(path);
    path.starts_with("/tmp/")
        || path.starts_with("/private/tmp/")
        || path.starts_with("/var/folders/")
        || path.contains("playwright_chromiumdev_profile")
        || path.contains("puppeteer_dev_chrome_profile")
}

fn process_role(process: &ProcessRecord, candidate: &Candidate<'_>) -> ProcessRole {
    if candidate.controller_pid == Some(process.pid()) {
        return ProcessRole::Controller;
    }
    if candidate.browser_root_pids.contains(&process.pid()) {
        return ProcessRole::BrowserRoot;
    }
    let basename = process.executable_basename().to_ascii_lowercase();
    if basename.contains("crashpad") || basename.contains("crash handler") {
        return ProcessRole::CrashHandler;
    }
    if matches_recorder(process, candidate.pack) {
        return ProcessRole::Recorder;
    }
    if let Some(arguments) = &process.arguments {
        for argument in arguments {
            let argument = argument.to_ascii_lowercase();
            if let Some(kind) = argument.strip_prefix("--type=") {
                return match kind {
                    "renderer" => ProcessRole::Renderer,
                    "gpu-process" => ProcessRole::Gpu,
                    "utility" => ProcessRole::Utility,
                    _ => ProcessRole::BrowserHelper,
                };
            }
        }
    }
    ProcessRole::IncidentMember
}

fn item(id: &str, family: EvidenceFamily, source_pid: Option<u32>) -> EvidenceItem {
    EvidenceItem {
        id: id.to_owned(),
        family,
        source_pid,
    }
}

fn sort_evidence(evidence: &mut Vec<EvidenceItem>) {
    evidence.sort_by(|left, right| {
        (left.family, left.id.as_str(), left.source_pid).cmp(&(
            right.family,
            right.id.as_str(),
            right.source_pid,
        ))
    });
    evidence.dedup_by(|left, right| {
        left.family == right.family && left.id == right.id && left.source_pid == right.source_pid
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use unlinger_core::{
        AppBundleVersion, ExecutableIdentity, ProcessIdentity, ProcessRuntimeFacts, ProcessStatus,
        SnapshotCoverage,
    };

    #[derive(Debug, Deserialize)]
    struct Corpus {
        schema_version: u32,
        cases: Vec<FixtureCase>,
    }

    #[derive(Debug, Deserialize)]
    struct FixtureCase {
        name: String,
        expected_state: IncidentState,
        expected_pack: Option<String>,
        second_observation: bool,
        processes: Vec<FixtureProcess>,
        #[serde(default)]
        second_processes: Vec<FixtureProcess>,
    }

    #[derive(Clone, Debug, Deserialize)]
    struct FixtureProcess {
        pid: u32,
        ppid: u32,
        pgid: u32,
        start: u64,
        name: String,
        exe: String,
        args: Vec<String>,
        #[serde(default = "default_uid")]
        uid: u32,
        #[serde(default)]
        tty_device: Option<u32>,
        #[serde(default)]
        app_bundle: Option<FixtureAppBundle>,
    }

    #[derive(Clone, Debug, Deserialize)]
    struct FixtureAppBundle {
        bundle_id: String,
        short_version: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SupportMatrix {
        schema_version: u32,
        platform: SupportPlatform,
        recognized_families: Vec<SupportedFamily>,
        artifacts: Vec<SupportedArtifact>,
        protocol: ProtocolSupport,
        acceptance: AcceptanceSupport,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SupportPlatform {
        source_target: SourceTarget,
        architectures: Vec<ArchitectureSupport>,
        universal_binary_verified: bool,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SourceTarget {
        os: String,
        minimum_major: u32,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ArchitectureSupport {
        id: String,
        source_verified: bool,
        controlled_field_verified: bool,
        release_verified: bool,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SupportedFamily {
        id: String,
        deterministic_classification: bool,
        automatic_process_eligibility: ProcessEligibility,
        always_protected: Vec<String>,
        synthetic_verified: bool,
        controlled_field_verified: bool,
        ambient_field_verified: bool,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ProcessEligibility {
        bundle_id: String,
        exact_versions: Vec<String>,
        controller_present: bool,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SupportedArtifact {
        kind: String,
        automatic_eligibility: String,
        current_rule_enabled: bool,
        synthetic_verified: bool,
        controlled_field_verified: bool,
        ambient_field_verified: bool,
        known_residuals: Vec<String>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ProtocolSupport {
        source_app_schema: u32,
        transitional_app_schema: u32,
        source_operator_schema: u32,
        historical_app_schema: u32,
        server_accepts: Vec<u32>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct AcceptanceSupport {
        private_enforcement_candidate_verified: bool,
        ambient_enforcement_accepted: bool,
        multi_day_dogfood_accepted: bool,
        public_release: bool,
    }

    fn default_uid() -> u32 {
        501
    }

    fn exact_cft_bundle() -> FixtureAppBundle {
        FixtureAppBundle {
            bundle_id: "com.google.chrome.for.testing".to_owned(),
            short_version: "151.0.7922.34".to_owned(),
        }
    }

    fn snapshot(processes: &[FixtureProcess], observed_at: u64) -> Snapshot {
        let records = processes
            .iter()
            .map(|process| ProcessRecord {
                identity: ProcessIdentity {
                    pid: process.pid,
                    started_at_unix_micros: process.start,
                    executable_device: Some(1),
                    executable_inode: Some(u64::from(process.pid) + process.start),
                },
                parent_pid: process.ppid,
                process_group_id: process.pgid,
                uid: process.uid,
                tty_device: process.tty_device,
                name: process.name.clone(),
                executable_path: Some(process.exe.clone()),
                executable: ExecutableIdentity {
                    device: Some(1),
                    inode: Some(u64::from(process.pid) + process.start),
                    size: Some(1),
                    modified_unix_nanos: Some(1),
                },
                arguments: Some(process.args.clone()),
                resident_memory_bytes: 1024,
                status: ProcessStatus::Sleeping,
                runtime: ProcessRuntimeFacts {
                    descriptor_facts_complete: true,
                    debug_transport_facts_complete: true,
                    app_bundle: process.app_bundle.as_ref().map(|bundle| AppBundleVersion {
                        bundle_id: bundle.bundle_id.clone(),
                        short_version: bundle.short_version.clone(),
                    }),
                    ..ProcessRuntimeFacts::default()
                },
            })
            .collect::<Vec<_>>();
        Snapshot {
            observed_at_unix_millis: observed_at,
            current_uid: 501,
            coverage: SnapshotCoverage {
                listed_processes: records.len(),
                inspected_processes: records.len(),
                ..SnapshotCoverage::default()
            },
            processes: records,
        }
    }

    fn analyzer() -> Analyzer {
        Analyzer::new(
            RuleSet::embedded().expect("embedded rules"),
            AnalyzerContext::default(),
        )
    }

    fn corpus_case(name: &str) -> FixtureCase {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../../../fixtures/macos/phase0-corpus.json"))
                .expect("valid corpus");
        corpus
            .cases
            .into_iter()
            .find(|case| case.name == name)
            .unwrap_or_else(|| panic!("missing fixture case {name:?}"))
    }

    fn fixture_process(
        pid: u32,
        ppid: u32,
        pgid: u32,
        start: u64,
        name: &str,
        executable: &str,
        arguments: &[&str],
    ) -> FixtureProcess {
        FixtureProcess {
            pid,
            ppid,
            pgid,
            start,
            name: name.to_owned(),
            exe: executable.to_owned(),
            args: arguments.iter().map(|value| (*value).to_owned()).collect(),
            uid: 501,
            tty_device: None,
            app_bundle: None,
        }
    }

    fn playwright_controller(pid: u32, ppid: u32, pgid: u32) -> FixtureProcess {
        fixture_process(
            pid,
            ppid,
            pgid,
            1_000_000,
            "node",
            "/synthetic/bin/node",
            &[
                "node",
                "/synthetic/node_modules/playwright/cli.js",
                "run-server",
            ],
        )
    }

    fn cft_browser(pid: u32, ppid: u32, pgid: u32, profile: &str) -> FixtureProcess {
        let mut process = fixture_process(
            pid,
            ppid,
            pgid,
            1_100_000,
            "Google Chrome for Testing",
            "/synthetic/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing",
            &[
                "Google Chrome for Testing",
                "--headless=new",
                "--remote-debugging-pipe",
                profile,
            ],
        );
        process.app_bundle = Some(exact_cft_bundle());
        process
    }

    fn ffmpeg(pid: u32, ppid: u32, pgid: u32) -> FixtureProcess {
        fixture_process(
            pid,
            ppid,
            pgid,
            1_200_000,
            "ffmpeg",
            "/opt/local/bin/ffmpeg",
            &["ffmpeg", "-f", "avfoundation"],
        )
    }

    #[test]
    fn embedded_packs_are_valid_and_unique() {
        let rules = RuleSet::embedded().expect("embedded packs are valid");
        assert_eq!(rules.packs().len(), 3);
        assert_eq!(
            rules
                .packs()
                .iter()
                .map(|pack| pack.id.as_str())
                .collect::<Vec<_>>(),
            vec!["agent-browser", "playwright", "puppeteer"]
        );
        assert!(rules.packs().iter().all(|pack| {
            pack.schema_version == 2
                && pack.version == "0.3.0"
                && pack.graceful_strategy == GracefulStrategy::OsTermOnly
                && pack.recorder_executable_basenames == ["ffmpeg"]
                && matches!(pack.version_policy, VersionPolicy::ExactAllowlist { .. })
        }));
        assert!(
            rules
                .packs()
                .iter()
                .all(|pack| !pack.artifact_policy.devtools_active_port)
        );
    }

    #[test]
    fn machine_readable_support_matrix_matches_embedded_authority() {
        let matrix: SupportMatrix =
            serde_json::from_str(include_str!("../../../docs/support-matrix.v1.json"))
                .expect("support matrix must decode strictly");
        let rules = RuleSet::embedded().expect("embedded rules");

        assert_eq!(matrix.schema_version, 1);
        assert_eq!(matrix.platform.source_target.os, "macos");
        assert_eq!(matrix.platform.source_target.minimum_major, 14);
        assert!(!matrix.platform.universal_binary_verified);
        assert_eq!(matrix.platform.architectures.len(), 2);
        let arm64 = matrix
            .platform
            .architectures
            .iter()
            .find(|architecture| architecture.id == "arm64")
            .expect("arm64 support row");
        assert!(arm64.source_verified);
        assert!(arm64.controlled_field_verified);
        assert!(!arm64.release_verified);
        let x86_64 = matrix
            .platform
            .architectures
            .iter()
            .find(|architecture| architecture.id == "x86_64")
            .expect("x86_64 support row");
        assert!(!x86_64.source_verified);
        assert!(!x86_64.controlled_field_verified);
        assert!(!x86_64.release_verified);

        let matrix_ids = matrix
            .recognized_families
            .iter()
            .map(|family| family.id.as_str())
            .collect::<Vec<_>>();
        let pack_ids = rules
            .packs()
            .iter()
            .map(|pack| pack.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(matrix_ids, pack_ids);
        for (family, pack) in matrix.recognized_families.iter().zip(rules.packs()) {
            assert!(family.deterministic_classification);
            assert!(family.synthetic_verified);
            assert!(!family.ambient_field_verified);
            assert!(!family.automatic_process_eligibility.controller_present);
            assert!(
                family
                    .always_protected
                    .iter()
                    .any(|shape| shape == "controller_bearing")
            );
            let VersionPolicy::ExactAllowlist {
                bundle_id,
                versions,
            } = &pack.version_policy
            else {
                panic!("{} must retain exact version admission", pack.id);
            };
            assert_eq!(family.automatic_process_eligibility.bundle_id, *bundle_id);
            assert_eq!(
                family.automatic_process_eligibility.exact_versions,
                *versions
            );
            assert_eq!(family.controlled_field_verified, family.id == "playwright");
        }

        assert_eq!(matrix.artifacts.len(), 1);
        let artifact = &matrix.artifacts[0];
        assert_eq!(artifact.kind, "devtools_active_port");
        assert_eq!(
            artifact.automatic_eligibility,
            "disabled_in_current_process_only_policy"
        );
        assert!(!artifact.current_rule_enabled);
        assert!(artifact.synthetic_verified);
        assert!(artifact.controlled_field_verified);
        assert!(!artifact.ambient_field_verified);
        assert_eq!(artifact.known_residuals.len(), 2);

        assert_eq!(matrix.protocol.source_app_schema, 4);
        assert_eq!(matrix.protocol.transitional_app_schema, 3);
        assert_eq!(matrix.protocol.source_operator_schema, 1);
        assert_eq!(matrix.protocol.historical_app_schema, 2);
        assert_eq!(matrix.protocol.server_accepts, [1, 3, 4]);
        assert!(matrix.acceptance.private_enforcement_candidate_verified);
        assert!(!matrix.acceptance.ambient_enforcement_accepted);
        assert!(!matrix.acceptance.multi_day_dogfood_accepted);
        assert!(!matrix.acceptance.public_release);
    }

    #[test]
    fn structured_version_policy_validates_exact_and_numeric_ranges() {
        let mut pack = RuleSet::embedded().expect("embedded packs").packs()[0].clone();
        assert_eq!(pack.schema_version, 2);
        assert!(matches!(
            pack.version_policy,
            VersionPolicy::ExactAllowlist { .. }
        ));

        pack.version_policy = VersionPolicy::BoundedRange {
            bundle_id: "com.google.chrome.for.testing".to_owned(),
            minimum_inclusive: "151.0.9.0".to_owned(),
            maximum_inclusive: "151.0.10.0".to_owned(),
        };
        pack.validate().expect("numeric ascending range");

        pack.version_policy = VersionPolicy::BoundedRange {
            bundle_id: "com.google.chrome.for.testing".to_owned(),
            minimum_inclusive: "151.0.10.0".to_owned(),
            maximum_inclusive: "151.0.9.0".to_owned(),
        };
        assert!(matches!(pack.validate(), Err(RuleError::InvalidPack(_))));

        pack.version_policy = VersionPolicy::ExactAllowlist {
            bundle_id: "com.google.chrome.for.testing".to_owned(),
            versions: vec!["151.0.7922.34".to_owned(), "151.0.7922.34".to_owned()],
        };
        assert!(matches!(pack.validate(), Err(RuleError::InvalidPack(_))));

        pack.version_policy = VersionPolicy::ExactAllowlist {
            bundle_id: "com.google.chrome.for.testing".to_owned(),
            versions: Vec::new(),
        };
        assert!(matches!(pack.validate(), Err(RuleError::InvalidPack(_))));

        pack.schema_version = 1;
        assert!(matches!(pack.validate(), Err(RuleError::InvalidPack(_))));

        let mut range_browser = cft_browser(
            99,
            1,
            99,
            "--user-data-dir=/private/tmp/playwright_chromiumdev_profile-range",
        );
        range_browser
            .app_bundle
            .as_mut()
            .expect("bundle fact")
            .short_version = "151.0.10.0".to_owned();
        let range_snapshot = snapshot(&[range_browser], 120_000);
        let policy = VersionPolicy::BoundedRange {
            bundle_id: "com.google.chrome.for.testing".to_owned(),
            minimum_inclusive: "151.0.9.0".to_owned(),
            maximum_inclusive: "151.0.10.0".to_owned(),
        };
        assert_eq!(
            evaluate_browser_versions(&[&range_snapshot.processes[0]], &policy),
            BrowserVersionGate::RangeSupported
        );
    }

    #[test]
    fn exact_browser_version_is_the_only_embedded_supported_point() {
        let case = corpus_case("abandoned Playwright browser without controller");
        let analyzer = analyzer();
        let exact = snapshot(&case.processes, 120_000);
        let exact_report = analyzer
            .observe(&exact)
            .expect("observe exact version")
            .into_iter()
            .find(|report| report.signature_pack == "playwright")
            .expect("Playwright report");
        assert_eq!(exact_report.state, IncidentState::Cooling);
        assert!(exact_report.evidence.iter().any(|item| {
            item.id == "version.browser_exact_allowlist"
                && item.family == EvidenceFamily::AutomationProvenance
        }));

        for (label, mutation, expected_evidence) in [
            ("missing", None, "protection.browser_version_missing"),
            (
                "unknown-product",
                Some(AppBundleVersion {
                    bundle_id: "org.chromium.Chromium".to_owned(),
                    short_version: "151.0.7922.34".to_owned(),
                }),
                "protection.browser_product_unsupported",
            ),
            (
                "wrong-version",
                Some(AppBundleVersion {
                    bundle_id: "com.google.chrome.for.testing".to_owned(),
                    short_version: "151.0.7922.35".to_owned(),
                }),
                "protection.browser_version_unsupported",
            ),
        ] {
            let mut changed = exact.clone();
            changed
                .processes
                .iter_mut()
                .find(|process| process.pid() == 300)
                .unwrap_or_else(|| panic!("{label} root"))
                .runtime
                .app_bundle = mutation;
            let report = analyzer
                .observe(&changed)
                .unwrap_or_else(|error| panic!("{label} observation failed: {error}"))
                .into_iter()
                .find(|report| report.signature_pack == "playwright")
                .unwrap_or_else(|| panic!("{label} missing report"));
            assert_eq!(report.state, IncidentState::Protected, "{label}");
            assert!(
                report
                    .evidence
                    .iter()
                    .any(|item| item.id == expected_evidence),
                "{label}: {:?}",
                report.evidence
            );
            assert!(report.runtime_artifacts.is_empty(), "{label}");
        }
    }

    #[test]
    fn browser_compatibility_and_support_catalog_are_rule_owned() {
        let case = corpus_case("abandoned Playwright browser without controller");
        let analyzer = analyzer();
        let exact = analyzer
            .observe(&snapshot(&case.processes, 120_000))
            .expect("observe exact version")
            .into_iter()
            .find(|report| report.signature_pack == "playwright")
            .expect("Playwright report");
        assert_eq!(
            exact.browser_compatibility,
            BrowserCompatibility {
                product: BrowserProduct::ChromeForTesting,
                observed_version: Some("151.0.7922.34".to_owned()),
                decision: BrowserCompatibilityDecision::Automatic,
                reason_id: None,
            }
        );

        let catalog = RuleSet::embedded()
            .expect("embedded rules")
            .browser_support_catalog();
        assert!(catalog.support_revision.starts_with("rules:"));
        assert_eq!(catalog.families.len(), 3);
        let playwright = catalog
            .families
            .iter()
            .find(|family| family.family == "playwright")
            .expect("Playwright support");
        assert_eq!(playwright.product, BrowserProduct::ChromeForTesting);
        assert_eq!(
            playwright.admitted_versions,
            vec!["151.0.7922.34".to_owned()]
        );
        assert_eq!(
            playwright.automatic_action_level,
            BrowserAutomaticActionLevel::Automatic
        );
    }

    #[test]
    fn observational_policy_is_report_only_and_a_version_change_blocks_revalidation() {
        let case = corpus_case("abandoned Playwright browser without controller");
        let exact_snapshot = snapshot(&case.processes, 120_000);
        let exact_analyzer = analyzer();
        let first = exact_analyzer
            .observe(&exact_snapshot)
            .expect("first exact observation");
        let second_snapshot = snapshot(&case.processes, 135_000);
        let second = exact_analyzer
            .observe(&second_snapshot)
            .expect("second exact observation");
        let confirmed_keys = second
            .iter()
            .map(|report| report.tracking_key.clone())
            .collect::<BTreeSet<_>>();
        let confirmed = exact_analyzer.reconcile_with_abandonment(&first, &second, &confirmed_keys);
        let report = confirmed
            .iter()
            .find(|report| report.signature_pack == "playwright")
            .expect("confirmed Playwright report");
        assert_eq!(report.state, IncidentState::Confirmed);
        let plan = CleanupPlan::from_confirmed(report).expect("confirmed plan");

        let mut incomplete_revival = second_snapshot.clone();
        incomplete_revival
            .processes
            .iter_mut()
            .find(|process| process.pid() == 300)
            .expect("browser root")
            .arguments = None;
        incomplete_revival.coverage.arguments_unavailable = 1;
        assert_eq!(
            exact_analyzer.revalidate(&incomplete_revival, &plan, RevalidationPhase::RevivalCheck,),
            Revalidation {
                status: RevalidationStatus::Blocked,
                reason_id: "revalidation.relevant_facts_incomplete".to_owned(),
            }
        );

        let unrelated_process = fixture_process(
            990,
            1,
            990,
            1_300_000,
            "unrelated-helper",
            "/synthetic/bin/unrelated-helper",
            &["unrelated-helper"],
        );
        let mut unrelated_incomplete = snapshot(&[unrelated_process], 136_000);
        let unrelated = &mut unrelated_incomplete.processes[0];
        unrelated.arguments = None;
        unrelated.executable.device = None;
        unrelated.executable.inode = None;
        unrelated.identity.executable_device = None;
        unrelated.identity.executable_inode = None;
        unrelated_incomplete.coverage.arguments_unavailable = 1;
        unrelated_incomplete
            .coverage
            .executable_identity_unavailable = 1;
        assert_eq!(
            exact_analyzer.revalidate(
                &unrelated_incomplete,
                &plan,
                RevalidationPhase::RevivalCheck,
            ),
            Revalidation {
                status: RevalidationStatus::Gone,
                reason_id: "revalidation.session_absent".to_owned(),
            }
        );

        let mut overlapping_changed_session = second_snapshot.clone();
        let changed_profile = overlapping_changed_session
            .processes
            .iter_mut()
            .find(|process| process.pid() == 300)
            .expect("browser root")
            .arguments
            .as_mut()
            .expect("browser arguments")
            .iter_mut()
            .find(|argument| argument.starts_with("--user-data-dir="))
            .expect("profile argument");
        *changed_profile =
            "--user-data-dir=/private/tmp/playwright_chromiumdev_profile-overlap-changed"
                .to_owned();
        let overlapping_report = exact_analyzer
            .observe(&overlapping_changed_session)
            .expect("observe changed overlapping session")
            .into_iter()
            .find(|report| {
                report.targets.iter().any(|target| {
                    target.identity.pid == plan.root_identity.pid
                        && target.identity.exact_match(&plan.root_identity)
                })
            })
            .expect("overlapping current report");
        assert_ne!(
            overlapping_report.session_fingerprint,
            plan.session_fingerprint
        );
        assert_eq!(
            exact_analyzer.revalidate(
                &overlapping_changed_session,
                &plan,
                RevalidationPhase::BeforeSignal,
            ),
            Revalidation {
                status: RevalidationStatus::Blocked,
                reason_id: "revalidation.overlapping_session_changed".to_owned(),
            }
        );

        let mut changed = second_snapshot;
        changed
            .processes
            .iter_mut()
            .find(|process| process.pid() == 300)
            .expect("browser root")
            .runtime
            .app_bundle
            .as_mut()
            .expect("bundle fact")
            .short_version = "151.0.7922.35".to_owned();
        assert_eq!(
            exact_analyzer.revalidate(&changed, &plan, RevalidationPhase::BeforeSignal),
            Revalidation {
                status: RevalidationStatus::Blocked,
                reason_id: "revalidation.confidence_downgraded".to_owned(),
            }
        );

        let mut observational_pack = RuleSet::embedded()
            .expect("embedded rules")
            .packs()
            .iter()
            .find(|pack| pack.id == "playwright")
            .expect("Playwright pack")
            .clone();
        observational_pack.version_policy = VersionPolicy::Observational;
        let observational = Analyzer::new(
            RuleSet {
                packs: vec![observational_pack],
            },
            AnalyzerContext::default(),
        );
        let report = observational
            .observe(&exact_snapshot)
            .expect("observe observational pack")
            .into_iter()
            .next()
            .expect("observational report");
        assert_eq!(report.state, IncidentState::Protected);
        assert!(
            report
                .evidence
                .iter()
                .any(|item| { item.id == "protection.version_observational_only" })
        );
    }

    #[test]
    fn minimum_candidate_age_is_a_hard_non_evidentiary_gate() {
        let case = corpus_case("abandoned Playwright browser without controller");
        let analyzer = analyzer();

        let too_young = analyzer
            .observe(&snapshot(&case.processes, 62_999))
            .expect("young observation")
            .into_iter()
            .find(|report| report.signature_pack == "playwright")
            .expect("young report");
        assert_eq!(too_young.state, IncidentState::Protected);
        assert!(too_young.evidence.iter().any(|item| {
            item.id == "protection.minimum_candidate_age_not_met"
                && item.family == EvidenceFamily::Protection
        }));

        let old_enough = analyzer
            .observe(&snapshot(&case.processes, 63_000))
            .expect("boundary observation")
            .into_iter()
            .find(|report| report.signature_pack == "playwright")
            .expect("boundary report");
        assert_eq!(old_enough.state, IncidentState::Cooling);
        assert!(!old_enough.evidence.iter().any(|item| {
            item.id.contains("candidate_age") || item.id.contains("minimum_candidate_age")
        }));

        let mut unknown_processes = case.processes.clone();
        unknown_processes[0].start = 0;
        let unknown = analyzer
            .observe(&snapshot(&unknown_processes, 120_000))
            .expect("unknown-age observation")
            .into_iter()
            .find(|report| report.signature_pack == "playwright")
            .expect("unknown-age report");
        assert_eq!(unknown.state, IncidentState::Protected);
        assert!(
            unknown
                .evidence
                .iter()
                .any(|item| item.id == "protection.candidate_age_unknown")
        );

        unknown_processes[0].start = 121_000_000;
        let underflow = analyzer
            .observe(&snapshot(&unknown_processes, 120_000))
            .expect("underflow observation")
            .into_iter()
            .find(|report| report.signature_pack == "playwright")
            .expect("underflow report");
        assert!(
            underflow
                .evidence
                .iter()
                .any(|item| item.id == "protection.candidate_age_unknown")
        );
    }

    #[test]
    fn controller_and_recorder_sessionization_is_bounded_and_fail_closed() {
        let controller = playwright_controller(800, 1, 800);
        let browser = cft_browser(
            801,
            800,
            800,
            "--user-data-dir=/private/tmp/playwright_chromiumdev_profile-recorder",
        );
        let joined = ffmpeg(802, 800, 800);
        let different_pgid = ffmpeg(803, 800, 803);
        let host = fixture_process(
            90,
            1,
            90,
            500_000,
            "Codex",
            "/Applications/Codex",
            &["Codex"],
        );
        let host_child = ffmpeg(804, 90, 800);
        let mut manual = ffmpeg(805, 800, 800);
        manual.tty_device = Some(1);
        let analyzer = analyzer();
        let reports = analyzer
            .observe(&snapshot(
                &[
                    host,
                    controller,
                    browser,
                    joined,
                    different_pgid,
                    host_child,
                    manual,
                ],
                120_000,
            ))
            .expect("observe recorder topology");
        let report = reports
            .iter()
            .find(|report| report.signature_pack == "playwright" && report.root.pid == 800)
            .expect("Playwright controller report");

        assert_eq!(report.state, IncidentState::Protected);
        assert!(
            report
                .evidence
                .iter()
                .any(|item| item.id == "abandonment.controller_reparented")
        );
        assert!(
            report
                .evidence
                .iter()
                .any(|item| item.id == "protection.controller_version_unverified")
        );
        assert!(
            report
                .evidence
                .iter()
                .any(|item| item.id == "protection.unjoined_recorder_residue")
        );
        assert_eq!(
            report
                .roles
                .iter()
                .find(|role| role.role == ProcessRole::Recorder)
                .map(|role| role.count),
            Some(1)
        );
        let target_pids = report
            .targets
            .iter()
            .map(|target| target.identity.pid)
            .collect::<BTreeSet<_>>();
        assert!(target_pids.contains(&802));
        assert!(!target_pids.contains(&803));
        assert!(!target_pids.contains(&804));
        assert!(!target_pids.contains(&805));
    }

    #[test]
    fn controller_only_is_visible_but_version_protected_and_never_confirms() {
        let controller = playwright_controller(900, 1, 900);
        let analyzer = analyzer();
        let first = analyzer
            .observe(&snapshot(std::slice::from_ref(&controller), 120_000))
            .expect("controller-only first observation");
        let report = first
            .iter()
            .find(|report| report.signature_pack == "playwright")
            .expect("controller-only report");
        assert_eq!(report.state, IncidentState::Protected);
        assert_eq!(report.member_count, 1);
        assert_eq!(report.targets[0].role, ProcessRole::Controller);
        assert!(
            report
                .evidence
                .iter()
                .any(|item| item.id == "ambiguity.browser_root_missing")
        );
        assert!(
            report
                .evidence
                .iter()
                .any(|item| item.id == "abandonment.controller_reparented")
        );
        assert!(
            report
                .evidence
                .iter()
                .any(|item| item.id == "protection.controller_version_unverified")
        );
        assert!(
            !report
                .evidence
                .iter()
                .any(|item| item.id == "provenance.headless_mode")
        );

        let second = analyzer
            .observe(&snapshot(std::slice::from_ref(&controller), 135_000))
            .expect("controller-only second observation");
        let keys = second
            .iter()
            .map(|report| report.tracking_key.clone())
            .collect::<BTreeSet<_>>();
        assert!(
            analyzer
                .reconcile_with_abandonment(&first, &second, &keys)
                .iter()
                .all(|report| report.state != IncidentState::Confirmed)
        );
    }

    #[test]
    fn mixed_browser_versions_protect_the_whole_controller_session() {
        let controller = playwright_controller(950, 1, 950);
        let first = cft_browser(
            951,
            950,
            950,
            "--user-data-dir=/private/tmp/playwright_chromiumdev_profile-mixed-a",
        );
        let mut second = cft_browser(
            952,
            950,
            950,
            "--user-data-dir=/private/tmp/playwright_chromiumdev_profile-mixed-b",
        );
        second
            .app_bundle
            .as_mut()
            .expect("bundle fact")
            .short_version = "151.0.7922.35".to_owned();
        let report = analyzer()
            .observe(&snapshot(&[controller, first, second], 120_000))
            .expect("observe mixed versions")
            .into_iter()
            .find(|report| report.signature_pack == "playwright")
            .expect("mixed report");
        assert_eq!(report.state, IncidentState::Protected);
        assert!(
            report
                .evidence
                .iter()
                .any(|item| item.id == "protection.browser_version_mixed")
        );
    }

    #[test]
    fn process_only_policy_does_not_admit_an_artifact_candidate() {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../../../fixtures/macos/phase0-corpus.json"))
                .expect("valid corpus");
        let case = corpus
            .cases
            .into_iter()
            .find(|case| case.name == "abandoned Playwright browser without controller")
            .expect("Playwright abandoned fixture");
        let analyzer = Analyzer::new(
            RuleSet::embedded().expect("rules"),
            AnalyzerContext::default(),
        );
        let reports = analyzer
            .observe(&snapshot(&case.processes, 120_000))
            .expect("observe fixture");
        let report = reports
            .iter()
            .find(|report| report.signature_pack == "playwright")
            .expect("Playwright report");

        assert!(report.runtime_artifacts.is_empty());
        let json = serde_json::to_string(report).expect("redacted report JSON");
        assert!(!json.contains("playwright_chromiumdev_profile-b"));
        assert!(!json.contains("runtime_artifacts"));
    }

    #[test]
    fn detached_crashpad_handler_is_not_a_browser_root_candidate() {
        let executable = concat!(
            "/Users/example/Library/Caches/ms-playwright/chromium-1234/",
            "chrome-mac-arm64/Google Chrome for Testing.app/Contents/Frameworks/",
            "Google Chrome for Testing Framework.framework/Versions/Current/Helpers/",
            "chrome_crashpad_handler"
        )
        .to_owned();
        let process = FixtureProcess {
            pid: 410,
            ppid: 1,
            pgid: 410,
            start: 10,
            name: "chrome_crashpad_handler".to_owned(),
            exe: executable.clone(),
            args: vec![
                executable,
                "--monitor-self-annotation=ptype=crashpad-handler".to_owned(),
                "--database=/private/tmp/playwright_chromiumdev_profile-case/Crashpad".to_owned(),
            ],
            uid: 501,
            tty_device: None,
            app_bundle: None,
        };
        let analyzer = Analyzer::new(
            RuleSet::embedded().expect("rules"),
            AnalyzerContext::default(),
        );

        let reports = analyzer
            .observe(&snapshot(&[process], 1_000))
            .expect("observe detached crashpad handler");

        assert!(reports.is_empty(), "crashpad helper became {reports:?}");
    }

    #[test]
    fn attached_debug_protocol_peer_protects_an_ephemeral_headless_session() {
        let executable = "/private/tmp/ms-playwright/Google Chrome for Testing".to_owned();
        let process = FixtureProcess {
            pid: 420,
            ppid: 1,
            pgid: 420,
            start: 20,
            name: "Google Chrome for Testing".to_owned(),
            exe: executable.clone(),
            args: vec![
                executable,
                "--headless=new".to_owned(),
                "--remote-debugging-port=9222".to_owned(),
                "--user-data-dir=/private/tmp/playwright_chromiumdev_profile-attached".to_owned(),
                "playwright".to_owned(),
            ],
            uid: 501,
            tty_device: None,
            app_bundle: Some(exact_cft_bundle()),
        };
        let mut snapshot = snapshot(&[process], 1_000);
        snapshot.processes[0].runtime.attached_debug_transport = true;
        let analyzer = Analyzer::new(
            RuleSet::embedded().expect("rules"),
            AnalyzerContext::default(),
        );

        let reports = analyzer.observe(&snapshot).expect("observe attached peer");

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].state, IncidentState::Protected);
        assert!(
            reports[0]
                .evidence
                .iter()
                .any(|item| item.id == "protection.attached_debug_peer")
        );
    }

    #[test]
    fn incomplete_debug_descriptor_visibility_fails_closed_without_serializing_runtime_facts() {
        let executable = "/private/tmp/ms-playwright/Google Chrome for Testing".to_owned();
        let process = FixtureProcess {
            pid: 421,
            ppid: 1,
            pgid: 421,
            start: 21,
            name: "Google Chrome for Testing".to_owned(),
            exe: executable.clone(),
            args: vec![
                executable,
                "--headless=new".to_owned(),
                "--remote-debugging-port=0".to_owned(),
                "--user-data-dir=/private/tmp/playwright_chromiumdev_profile-incomplete".to_owned(),
                "playwright".to_owned(),
            ],
            uid: 501,
            tty_device: None,
            app_bundle: Some(exact_cft_bundle()),
        };
        let mut snapshot = snapshot(&[process], 1_000);
        snapshot.processes[0].runtime.debug_transport_facts_complete = false;
        snapshot.processes[0].runtime.tcp_established_local_ports = vec![49152];
        let analyzer = Analyzer::new(
            RuleSet::embedded().expect("rules"),
            AnalyzerContext::default(),
        );

        let reports = analyzer
            .observe(&snapshot)
            .expect("observe incomplete descriptor facts");
        let serialized = serde_json::to_value(&snapshot.processes[0]).expect("serialize process");

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].state, IncidentState::Protected);
        assert!(
            reports[0]
                .evidence
                .iter()
                .any(|item| { item.id == "protection.debug_peer_visibility_incomplete" })
        );
        assert!(serialized.get("runtime").is_none());
        assert!(!serialized.to_string().contains("49152"));
    }

    #[test]
    fn corpus_enforces_positive_and_nearest_counterexamples() {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../../../fixtures/macos/phase0-corpus.json"))
                .expect("valid corpus");
        assert_eq!(corpus.schema_version, 2);
        let analyzer = Analyzer::new(
            RuleSet::embedded().expect("rules"),
            AnalyzerContext::default(),
        );

        for case in corpus.cases {
            let first = analyzer
                .observe(&snapshot(&case.processes, 120_000))
                .unwrap_or_else(|error| panic!("{} first observation failed: {error}", case.name));
            let second_source = if case.second_processes.is_empty() {
                &case.processes
            } else {
                &case.second_processes
            };
            let reports = if case.second_observation {
                let second = analyzer
                    .observe(&snapshot(second_source, 135_000))
                    .unwrap_or_else(|error| {
                        panic!("{} second observation failed: {error}", case.name)
                    });
                let confirmed_tracking_keys = second
                    .iter()
                    .filter(|report| report.state == IncidentState::Cooling)
                    .map(|report| report.tracking_key.clone())
                    .collect::<BTreeSet<_>>();
                analyzer.reconcile_with_abandonment(&first, &second, &confirmed_tracking_keys)
            } else {
                first
            };

            if let Some(expected_pack) = case.expected_pack {
                let report = reports
                    .iter()
                    .find(|report| report.signature_pack == expected_pack)
                    .unwrap_or_else(|| panic!("{} missing {expected_pack} report", case.name));
                assert_eq!(report.state, case.expected_state, "{}", case.name);
            } else {
                assert!(
                    reports
                        .iter()
                        .all(|report| report.state != IncidentState::Confirmed),
                    "{} must never become confirmed",
                    case.name
                );
            }
        }
    }

    #[test]
    fn stable_observations_do_not_confirm_without_external_abandonment_grace() {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../../../fixtures/macos/phase0-corpus.json"))
                .expect("valid corpus");
        let case = corpus
            .cases
            .into_iter()
            .find(|case| case.name == "abandoned agent-browser Chrome-for-Testing tree")
            .expect("fixture case");
        let analyzer = Analyzer::new(
            RuleSet::embedded().expect("rules"),
            AnalyzerContext::default(),
        );
        let first = analyzer
            .observe(&snapshot(&case.processes, 120_000))
            .expect("first observation");
        let second = analyzer
            .observe(&snapshot(&case.processes, 135_000))
            .expect("second observation");
        let reports = analyzer.reconcile(&first, &second);

        assert_eq!(reports[0].state, IncidentState::Cooling);
        assert!(reports[0].gates.stable_across_two_observations);
        assert!(!reports[0].gates.confirmed_abandonment);
    }
}
