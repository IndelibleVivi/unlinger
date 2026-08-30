use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use unlinger_core::{
    CleanupPlan, EvidenceFamily, EvidenceItem, GateLedger, IncidentReport, IncidentRevalidator,
    IncidentState, ProcessGraph, ProcessRecord, ProcessRole, ProcessRoleCount, ProcessTarget,
    Revalidation, RevalidationPhase, RevalidationStatus, RootSummary, Snapshot, fingerprint_parts,
};

const STANDARD_PROFILE_MARKERS: &[&str] = &[
    "/library/application support/google/chrome",
    "/library/application support/chromium",
    "/library/application support/microsoft edge",
];

const HEADLESS_MARKERS: &[&str] = &["--headless", "--headless=new", "--headless=old"];
const TRANSPORT_MARKERS: &[&str] = &["--remote-debugging-pipe", "--remote-debugging-port"];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SignaturePack {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub supported_versions: String,
    pub controller_markers: Vec<String>,
    pub framework_markers: Vec<String>,
    pub ephemeral_profile_markers: Vec<String>,
    pub browser_executable_markers: Vec<String>,
}

impl SignaturePack {
    fn validate(&self) -> Result<(), RuleError> {
        if self.schema_version != 1 {
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
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct RuleSet {
    packs: Vec<SignaturePack>,
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
            .filter_map(|candidate| self.classify(&graph, candidate))
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
            let controllers = graph
                .processes()
                .filter(|process| matches_controller(process, pack))
                .map(ProcessRecord::pid)
                .collect::<BTreeSet<_>>();
            let browser_roots = graph
                .processes()
                .filter(|process| is_browser_root(process, pack))
                .map(ProcessRecord::pid)
                .collect::<Vec<_>>();

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

            for (controller_pid, mut browsers) in by_controller {
                browsers.sort_unstable();
                let mut members = vec![controller_pid];
                for browser_pid in &browsers {
                    members.extend(graph.descendant_pids(*browser_pid));
                }
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

    fn classify(&self, graph: &ProcessGraph, candidate: &Candidate<'_>) -> Option<IncidentReport> {
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
        let framework_argument = members
            .iter()
            .any(|process| process_contains_any(process, &candidate.pack.framework_markers));
        let headless = browser_roots
            .iter()
            .all(|process| args_contain_any(process, HEADLESS_MARKERS));
        let transport = browser_roots
            .iter()
            .any(|process| args_contain_any(process, TRANSPORT_MARKERS));
        let debug_pipe = browser_roots
            .iter()
            .any(|process| args_contain(process, "--remote-debugging-pipe"));
        let chrome_for_testing = browser_roots.iter().any(|process| {
            process
                .executable_path
                .as_deref()
                .unwrap_or(&process.name)
                .to_ascii_lowercase()
                .contains("chrome for testing")
                || process
                    .executable_path
                    .as_deref()
                    .unwrap_or(&process.name)
                    .to_ascii_lowercase()
                    .contains("chrome-headless-shell")
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
        if !headless {
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
        if chrome_for_testing {
            provenance_categories += 1;
            evidence.push(item(
                "provenance.automation_browser_binary",
                EvidenceFamily::AutomationProvenance,
                candidate.browser_root_pids.first().copied(),
            ));
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
        let owner_missing = candidate.controller_pid.is_none()
            && browser_roots
                .iter()
                .all(|browser| browser.parent_pid == 1 || graph.get(browser.parent_pid).is_none());
        if controller_parent_live {
            evidence.push(item(
                "abandonment.live_controller_owner",
                EvidenceFamily::Abandonment,
                candidate.controller_pid,
            ));
        } else if controller_reparented {
            evidence.push(item(
                "abandonment.controller_reparented_unproven",
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

        let root_identity_row = format!(
            "{}:{}:{}:{}",
            root.identity.pid,
            root.identity.started_at_unix_micros,
            root.identity.executable_device.unwrap_or_default(),
            root.identity.executable_inode.unwrap_or_default()
        );
        let identity_fingerprint = fingerprint_parts([root_identity_row.as_bytes()]);
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
            targets,
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

fn matches_controller(process: &ProcessRecord, pack: &SignaturePack) -> bool {
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
    if basename == "ffmpeg" || basename.contains("replayd") {
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
    use unlinger_core::{ExecutableIdentity, ProcessIdentity, ProcessStatus, SnapshotCoverage};

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
    }

    fn default_uid() -> u32 {
        501
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
                tty_device: None,
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
    fn corpus_enforces_positive_and_nearest_counterexamples() {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../../../fixtures/macos/phase0-corpus.json"))
                .expect("valid corpus");
        assert_eq!(corpus.schema_version, 1);
        let analyzer = Analyzer::new(
            RuleSet::embedded().expect("rules"),
            AnalyzerContext::default(),
        );

        for case in corpus.cases {
            let first = analyzer
                .observe(&snapshot(&case.processes, 1_000))
                .unwrap_or_else(|error| panic!("{} first observation failed: {error}", case.name));
            let second_source = if case.second_processes.is_empty() {
                &case.processes
            } else {
                &case.second_processes
            };
            let reports = if case.second_observation {
                let second = analyzer
                    .observe(&snapshot(second_source, 16_000))
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
            .observe(&snapshot(&case.processes, 1_000))
            .expect("first observation");
        let second = analyzer
            .observe(&snapshot(&case.processes, 16_000))
            .expect("second observation");
        let reports = analyzer.reconcile(&first, &second);

        assert_eq!(reports[0].state, IncidentState::Cooling);
        assert!(reports[0].gates.stable_across_two_observations);
        assert!(!reports[0].gates.confirmed_abandonment);
    }
}
