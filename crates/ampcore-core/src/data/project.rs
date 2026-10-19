use serde::{Deserialize, Serialize};
use specta::Type;

use super::capability::{CrossoverFilterType, EqFilterType, ParamRange, PowerMode, SourceKind};
use super::common::{new_id, now_millis};
use crate::live::cvr::fir::{taps_summary, trimmed_taps};

/// Which physical input feeds a channel — a `SourceKind` alone isn't enough
/// to identify one, since a model typically exposes several physical inputs
/// per kind (e.g. 4 analog inputs on a 4-channel amp); `index` picks which
/// one (0.. the kind's `SourceChannelCount.channel_count`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSource {
    pub kind: SourceKind,
    pub index: u32,
}

/// One crosspoint in a channel's row of the input matrix — the gain/active
/// state for a single source position. `source_index` is positional (0..
/// `AmpDspTopology.matrix_input_count`), not a `SourceKind` itself, since a
/// model can have multiple sources of the same kind (e.g. 4 analog + 4 Dante
/// inputs on a Dante-equipped model).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MatrixCrosspoint {
    pub source_index: u32,
    pub gain_db: f64,
    pub active: bool,
}

/// One HP or LP crossover slot (band 0 / band 9 of the fixed 10-band chain —
/// see `EQ_BANDS_PER_CHANNEL`). No `q` field: Q is implied entirely by
/// `filter_type` (e.g. "BW-12" always means Q=1/sqrt(2), never a
/// user-settable independent value), confirmed against the old app's
/// reference UI. Has `active` — HP/LP slots are independently bypassable.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CrossoverSlot {
    pub filter_type: CrossoverFilterType,
    pub freq_hz: f64,
    pub active: bool,
}

/// One of the 8 parametric bands (bands 1-8 of the fixed 10-band chain).
/// `active` mirrors `MatrixCrosspoint.active` — lets a band be fully
/// disengaged independent of its stored gain/freq/Q.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EqBand {
    pub filter_type: EqFilterType,
    pub freq_hz: f64,
    pub gain_db: f64,
    pub q: f64,
    pub active: bool,
}

/// A full 10-band EQ chain (HP crossover + 8 parametric bands + LP
/// crossover) for one channel, one direction. `AmpChannel` holds two of
/// these (`input_eq`/`output_eq`) — Input/Output tabs' EQ sub-tabs.
/// `bands.len()` == `AmpDspTopology.eq_bands_per_channel - 2`; grown/shrunk
/// alongside `channels` whenever the assigned model's topology changes, see
/// `reconcile_eq_bands`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChannelEq {
    pub hp: CrossoverSlot,
    pub bands: Vec<EqBand>,
    pub lp: CrossoverSlot,
}

/// Which of a channel's two independent 10-band EQ chains a command targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum EqDirection {
    Input,
    Output,
}

/// Which crossover slot (band 0 or band 9) a command targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum CrossoverSlotKind {
    Hp,
    Lp,
}

/// Partial update for one `EqBand` — bundled into a single struct param
/// (rather than 5 separate `Option<T>` params) because `projects_set_eq_band`
/// otherwise exceeds `tauri-specta`'s `SpectaFn` 10-parameter limit. Every
/// field `None` means "leave unchanged", same semantics as the flat
/// `Option<T>` params every other partial-update command uses.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EqBandPatch {
    pub filter_type: Option<EqFilterType>,
    pub freq_hz: Option<f64>,
    pub gain_db: Option<f64>,
    pub q: Option<f64>,
    pub active: Option<bool>,
}

/// Partial update for a `CrossoverSlot` — same bundling rationale as
/// `EqBandPatch`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CrossoverSlotPatch {
    pub filter_type: Option<CrossoverFilterType>,
    pub freq_hz: Option<f64>,
    pub active: Option<bool>,
}

/// RMS-window limiter stage — ranged by `AmpParamRanges.rms_limiter_*`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RmsLimiter {
    pub enabled: bool,
    pub threshold_vrms: f64,
    pub attack_ms: f64,
    pub release_multiplier: f64,
    /// Vendor `RMS_Limiter_Auto`. Read back, not yet editable here.
    #[serde(default)]
    pub auto: bool,
    /// Vendor `RMS_Limiter_max` (V). Read back, not yet editable here.
    #[serde(default)]
    pub max_vrms: f64,
}

/// Instantaneous peak limiter stage — ranged by `AmpParamRanges.peak_limiter_*`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PeakLimiter {
    pub enabled: bool,
    pub threshold_vp: f64,
    pub hold_ms: f64,
    pub release_ms: f64,
    /// Vendor `peak_Limiter_max` (V). Read back, not yet editable here.
    #[serde(default)]
    pub max_vp: f64,
}

/// A channel's output protection: independent RMS and Peak limiter stages,
/// ported from the old app's `limiter-panel.tsx` (both stages can be engaged
/// simultaneously, each with its own enable flag).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Limiter {
    pub rms: RmsLimiter,
    pub peak: PeakLimiter,
}

/// Partial update for a `Limiter` — bundled into a single struct param for
/// the same `SpectaFn` 10-parameter reason as `EqBandPatch`/`CrossoverSlotPatch`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct LimiterPatch {
    pub rms_enabled: Option<bool>,
    pub rms_threshold_vrms: Option<f64>,
    pub rms_attack_ms: Option<f64>,
    pub rms_release_multiplier: Option<f64>,
    pub peak_enabled: Option<bool>,
    pub peak_threshold_vp: Option<f64>,
    pub peak_hold_ms: Option<f64>,
    pub peak_release_ms: Option<f64>,
}

/// One trim + delay pair for a single input source kind.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceTrim {
    pub trim_db: f64,
    pub delay_ms: f64,
}

/// Per-source gain matching (vendor `Ana/Dante_gain_matching`) — each source
/// kind feeding a channel has its own trim and delay, independent of which
/// source is currently selected.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceTrims {
    pub analog: SourceTrim,
    pub dante: SourceTrim,
}

/// Partial update for one `SourceTrim` — same bundling rationale as
/// `EqBandPatch`. Either half may be omitted, but the wire has no partial
/// form (FC=62 always carries both), so a writer that sets one must re-send
/// the sibling's current value.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceTrimPatch {
    pub trim_db: Option<f64>,
    pub delay_ms: Option<f64>,
}

/// Partial update for a `BackupPriority` — same bundling rationale as
/// `EqBandPatch`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupPriorityPatch {
    pub enabled: Option<bool>,
    pub first: Option<u8>,
    pub second: Option<u8>,
    pub threshold_db: Option<i32>,
}

/// Backup source switching for one channel (vendor `StruPriority`). `first`/
/// `second` are the raw source codes as stored on the amp — which kind a code
/// names depends on the model's source set, so they are not mapped here.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BackupPriority {
    pub enabled: bool,
    pub first: u8,
    pub second: u8,
    pub threshold_db: i32,
}

/// Per-channel config on an amp assignment. `ohms` is the channel's load
/// impedance, used by the Limiter; the amp reports its own as `load_data`.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AmpChannel {
    pub channel_index: u32,
    pub ohms: f64,
    /// Which physical source feeds this channel's input — Routing tab.
    /// Always set: a physical input always has a source. New channels
    /// default to Analog N (1:1).
    pub source: ChannelSource,
    /// One crosspoint per possible matrix source (0..matrix_input_count) —
    /// Matrix tab. Grown/shrunk alongside `channels` whenever the assigned
    /// model (hence its topology) changes; see `reconcile_matrix_size`.
    #[serde(default)]
    pub matrix_crosspoints: Vec<MatrixCrosspoint>,
    /// Input delay, ranged by `AmpParamRanges.delay_in_ms` — Input tab.
    #[serde(default)]
    pub delay_in_ms: f64,
    /// Whether this channel's input is muted — Input tab.
    #[serde(default)]
    pub input_muted: bool,
    /// Output trim, ranged by `AmpParamRanges.output_trim_db` — Output tab.
    #[serde(default)]
    pub output_trim_db: f64,
    /// Output volume, ranged by `AmpParamRanges.output_volume_db` — Output tab.
    #[serde(default)]
    pub output_volume_db: f64,
    /// Output delay, ranged by `AmpParamRanges.delay_out_ms` — Output tab.
    #[serde(default)]
    pub delay_out_ms: f64,
    /// Input EQ tab's 10-band chain (pre-matrix).
    #[serde(default = "default_channel_eq")]
    pub input_eq: ChannelEq,
    /// Output EQ tab's 10-band chain (post-matrix), independent of `input_eq`.
    #[serde(default = "default_channel_eq")]
    pub output_eq: ChannelEq,
    /// Output protection — RMS + Peak limiter stages, Output tab's Limiter
    /// sub-tab.
    #[serde(default = "default_limiter")]
    pub limiter: Limiter,
    /// Whether the output noise gate is engaged — Output tab. Threshold is
    /// only user-adjustable in the UI when
    /// `CvrFirmwareCapability.noise_gate_threshold` is true, but is always
    /// persisted regardless of firmware.
    #[serde(default)]
    pub noise_gate_enabled: bool,
    /// Noise gate threshold in dBu, ranged by
    /// `AmpParamRanges.noise_gate_threshold_dbu` — Output tab.
    #[serde(default)]
    pub noise_gate_threshold_dbu: f64,
    /// Output polarity/phase invert — Output tab.
    #[serde(default)]
    pub output_phase_inverted: bool,
    /// User-assigned label for this channel's input side, overriding the
    /// default "In{n}" label. `None` uses the default. Max length ranged by
    /// `AmpParamRanges.channel_name_max_length`.
    #[serde(default)]
    pub input_name: Option<String>,
    /// User-assigned label for this channel's output side, overriding the
    /// default "Out{letter}" label. `None` uses the default.
    #[serde(default)]
    pub output_name: Option<String>,
    /// Whether this channel's output is muted — Output tab. Mirrors
    /// `input_muted`.
    #[serde(default)]
    pub output_muted: bool,
    /// Whether this channel is mono-bridged with the next channel (fixed
    /// adjacent pairing: `floor(channel_index / 2)` — (0,1), (2,3), …) —
    /// Output tab. **Only meaningful on an even-indexed (pair-leader)
    /// channel with a following odd-indexed partner**; ignored/never read
    /// on odd (follower) channels or on a leader with no partner (e.g. the
    /// trailing channel of an odd-count assignment). Ported from the old
    /// app's per-pair `BridgeReadback`, adapted to a plain persisted field
    /// since this is offline planning, not a live device readback.
    #[serde(default)]
    pub output_bridged: bool,
    /// Output power/impedance mode — Output tab. Ranged by
    /// `AmpDspTopology.power_modes` (which modes the assigned model actually
    /// offers), though CVR currently offers all three unconditionally.
    #[serde(default = "default_power_mode")]
    pub power_mode: PowerMode,
    /// FIR filter bypass. Read back, not yet editable here.
    #[serde(default)]
    pub fir_bypassed: bool,
    /// Per-source trim/delay — each source keeps its own pair, independent of
    /// which one is currently selected. Routing tab.
    #[serde(default)]
    pub source_trims: SourceTrims,
    /// Backup source switching — Routing tab, offered only on amps with a
    /// second source to fail over to (`AmpModelCatalogEntry.is_dante`).
    #[serde(default)]
    pub backup_priority: BackupPriority,
    /// The speaker-library way this output was set up from, if any — a
    /// reference only. The values themselves live in the fields above, so the
    /// project never needs the library to open or push; see `data/speaker.rs`.
    #[serde(default)]
    pub speaker: Option<SpeakerRef>,
    /// The FIR filter this output holds; a unit impulse when none is loaded,
    /// as on the amp. Hashed (taps only) and pushed like every other setting;
    /// the amp's own comes from the driver's FC=43 reads, not from FC=27.
    #[serde(default = "ChannelFir::identity")]
    pub fir: ChannelFir,
}

/// An output's FIR filter: the amp's 32-byte name and its taps (at most
/// `fir::FIR_MAX_TAPS`; shorter is zero-padded on the wire).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ChannelFir {
    pub name: String,
    pub coefficients: Vec<f32>,
}

impl ChannelFir {
    /// What an amp holds with no filter loaded: a unit impulse.
    pub fn identity() -> Self {
        Self { name: String::new(), coefficients: vec![1.0] }
    }

    /// Whether these are this filter's taps, bit for bit and without the
    /// zeros the wire pads to 512 with (they are stored and sent as the same
    /// f32s). The name is a label, not compared — an amp answering with the
    /// nameless reply form can't report one, and the fingerprint doesn't hash
    /// it either.
    pub fn same_taps(&self, coefficients: &[f32]) -> bool {
        let bits = |taps: &[f32]| trimmed_taps(taps).iter().map(|t| t.to_bits()).collect::<Vec<_>>();
        bits(&self.coefficients) == bits(coefficients)
    }

    /// "N taps · checksum" — what status, the comparator and the fingerprint
    /// rows compare. Taps only, like `same_taps`.
    pub fn summary(&self) -> String {
        taps_summary(&self.coefficients)
    }
}

/// Which library way an output was set up from, and which revision of it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerRef {
    pub library_id: String,
    pub way_index: u32,
    pub revision: u32,
    /// "Brand Model · Way" at assign time, so a reference whose entry was
    /// deleted from this machine's library still reads as something.
    pub label: String,
    /// The `ProjectSpeaker` this output is a way of. `None` for a speaker set
    /// up on the amp itself, which is not a project speaker. An id no project
    /// speaker has reads the same.
    #[serde(default)]
    pub project_speaker_id: Option<String>,
}

/// A speaker placed in the project, before or after its ways are patched to
/// amp outputs. It stores no links: its linked ways are the outputs — of any
/// amp — whose `SpeakerRef.project_speaker_id` is its id, so removing an amp
/// or shrinking it can't leave a link behind.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSpeaker {
    pub id: String,
    pub name: String,
    pub library_id: String,
    /// "Brand Model" at add time, so a speaker whose entry was deleted from
    /// this machine's library still reads as something.
    pub label: String,
    /// Where the user put it on the Workspace canvas; `None` until it has
    /// been dragged, and the canvas picks a free spot.
    #[serde(default)]
    pub position: Option<CanvasPosition>,
    /// Identical cabinets wired in parallel on the same outputs, as
    /// ArmoníaPlus's Parallel mode: the group is one speaker, linked once —
    /// each way drives `parallel` boxes. 1 = a single cabinet; at most
    /// `MAX_PARALLEL`. Descriptive only: nothing is written per cabinet.
    #[serde(default = "one_cabinet")]
    pub parallel: u32,
}

/// The largest parallel group a project speaker can be.
pub const MAX_PARALLEL: u32 = 8;

fn one_cabinet() -> u32 {
    1
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Type)]
pub struct CanvasPosition {
    pub x: f64,
    pub y: f64,
}

impl ProjectSpeaker {
    pub fn new(name: String, library_id: String, label: String, parallel: u32) -> Self {
        Self { id: new_id(), name, library_id, label, position: None, parallel }
    }
}

/// Project speakers run from one control: a gain, a delay and a mute for all
/// of them. A speaker is in at most one group.
///
/// The group holds no channel of its own. Its gain and delay are offsets that
/// have already been added to its members' outputs — gain to the output
/// volume, delay to the output delay — so the push, the fingerprint and the
/// amp know nothing of groups (`Project::shift_speakers`). The delay part is
/// taken back out wherever an output is compared with or saved as its library
/// way (`Project::group_delay_ms`), so a grouped speaker still reads "Match".
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerGroup {
    pub id: String,
    pub name: String,
    pub speaker_ids: Vec<String>,
    /// Where its control sits on the Workspace canvas; `None` until dragged.
    #[serde(default)]
    pub position: Option<CanvasPosition>,
    #[serde(default)]
    pub gain_db: f64,
    #[serde(default)]
    pub delay_ms: f64,
    /// Every member output is muted. Unmuting unmutes them all.
    #[serde(default)]
    pub muted: bool,
}

impl SpeakerGroup {
    pub fn new(name: String) -> Self {
        Self { id: new_id(), name, speaker_ids: Vec::new(), position: None, gain_db: 0.0, delay_ms: 0.0, muted: false }
    }
}

/// The ranges a group's shift must keep an amp's outputs in.
#[derive(Debug, Clone, Copy)]
pub struct OutputLimits {
    pub volume_db: ParamRange,
    pub delay_out_ms: ParamRange,
}

/// What `Project::shift_speakers` changed: the offsets it could apply (the
/// asked ones, cut so no output leaves its range) and the amps it touched.
#[derive(Debug, Clone, PartialEq)]
pub struct Shifted {
    pub gain_db: f64,
    pub delay_ms: f64,
    pub amp_ids: Vec<String>,
}

/// One assigned amp "slot" within a Project. `id` is independent of `mac` so
/// a slot's configuration survives a physical unit swap. `mac` starts unset
/// at creation time — a slot is planned by model/name alone; linking it to
/// a physical unit's MAC happens later via live network discovery, not by
/// manual entry.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AmpAssignment {
    pub id: String,
    pub mac: Option<String>,
    pub amp_model_id: Option<String>,
    /// Firmware version this slot is being planned for (e.g. "1.1.8",
    /// "1.1.9") — declared by the user at planning time, not detected, since
    /// offline planning has no live device to sniff it from. Free text
    /// (mirrors `DiscoveredDevice.firmware_family`) rather than a closed
    /// enum, since it's protocol-specific and protocols are pluggable.
    #[serde(default)]
    pub firmware_version: Option<String>,
    pub channels: Vec<AmpChannel>,
    /// The amp's user-set name (FC=60 `CUSTOMER_NAME_MODIFY`, read back via FC=0
    /// BASIC_INFO). Editable offline here, or live via
    /// `live_control_set_device_name` — the one name an amp has, whether
    /// planned or connected.
    #[serde(default)]
    pub device_name: Option<String>,
    /// Manual override: this amp is linked to hardware that may well be
    /// online, but the user has deliberately stepped out of the live session.
    /// The plan then edits exactly as it does for an offline amp, and nothing
    /// is written to the amp. Persisted here rather than held in the UI so it
    /// survives the amp dropping offline and coming back — the Offline banner
    /// takes over while it is away, and the Disengaged one returns with it.
    /// See `edit_lock::AmpEditLockState::Disengaged`.
    #[serde(default)]
    pub live_disengaged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub schema_version: u32,
    pub created_at: f64,
    pub updated_at: f64,
    pub amp_assignments: Vec<AmpAssignment>,
    #[serde(default)]
    pub speakers: Vec<ProjectSpeaker>,
    #[serde(default)]
    pub groups: Vec<SpeakerGroup>,
}

/// A version stamp only — nothing reads it to gate behavior. There are no
/// released builds yet, so there is no old-file compatibility to preserve and
/// no migration functions exist; a struct change that would otherwise break
/// deserialization is just made directly. New fields still need
/// `#[serde(default = ...)]` (see this file's module doc) purely so a
/// project file saved earlier this session keeps loading after a field is
/// added mid-development, not for any long-term compatibility guarantee.
///
/// 3: `AmpChannel.fir` — absent in older files, which backfills to a unit
/// impulse (no filter loaded).
///
/// 4: `Project.speakers` and `SpeakerRef.project_speaker_id` — absent in
/// older files, which backfills to no project speakers: every output's
/// speaker stays one set up on its amp. `ProjectSpeaker.position` backfills
/// to unplaced.
///
/// 5: `ProjectSpeaker.parallel` — absent in older files, which backfills to 1
/// (a single cabinet).
///
/// 6: `Project.groups` — absent in older files, which backfills to no groups.
pub const CURRENT_PROJECT_SCHEMA_VERSION: u32 = 6;

impl Project {
    pub fn new(name: String, description: String) -> Self {
        let now = now_millis();
        Self {
            id: new_id(),
            name,
            description,
            schema_version: CURRENT_PROJECT_SCHEMA_VERSION,
            created_at: now,
            updated_at: now,
            amp_assignments: Vec::new(),
            speakers: Vec::new(),
            groups: Vec::new(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = now_millis();
    }

    /// The group a project speaker is in, if any.
    pub fn group_of(&self, speaker_id: &str) -> Option<&SpeakerGroup> {
        self.groups.iter().find(|g| g.speaker_ids.iter().any(|id| id == speaker_id))
    }

    /// The delay a project speaker's group has added to its outputs; 0 for
    /// an output that is no project speaker's or one in no group.
    pub fn group_delay_ms(&self, project_speaker_id: Option<&str>) -> f64 {
        project_speaker_id.and_then(|id| self.group_of(id)).map_or(0.0, |g| g.delay_ms)
    }

    /// Adds `gain_db` to the output volume and `delay_ms` to the output delay
    /// of every output linked to these speakers, and mutes or unmutes them
    /// with `mute`. Both offsets are first cut so that no output leaves its
    /// amp's range (`limits`; an amp without one only keeps its delay at or
    /// above 0) — every output then moves by the same amount, and shifting
    /// back by what was applied restores them exactly.
    pub fn shift_speakers(
        &mut self,
        speaker_ids: &[String],
        gain_db: f64,
        delay_ms: f64,
        mute: Option<bool>,
        limits: impl Fn(&AmpAssignment) -> Option<OutputLimits>,
    ) -> Shifted {
        let member = |c: &AmpChannel| {
            c.speaker.as_ref().and_then(|s| s.project_speaker_id.as_ref()).is_some_and(|id| speaker_ids.contains(id))
        };
        let (mut gain_lo, mut gain_hi) = (f64::NEG_INFINITY, f64::INFINITY);
        let (mut delay_lo, mut delay_hi) = (f64::NEG_INFINITY, f64::INFINITY);
        for amp in &self.amp_assignments {
            let limits = limits(amp);
            for channel in amp.channels.iter().filter(|c| member(c)) {
                let delay = limits.map_or(ParamRange { min: 0.0, max: f64::INFINITY }, |l| l.delay_out_ms);
                delay_lo = delay_lo.max(delay.min - channel.delay_out_ms);
                delay_hi = delay_hi.min(delay.max - channel.delay_out_ms);
                if let Some(l) = limits {
                    gain_lo = gain_lo.max(l.volume_db.min - channel.output_volume_db);
                    gain_hi = gain_hi.min(l.volume_db.max - channel.output_volume_db);
                }
            }
        }
        // An output already outside its range leaves no room either way.
        let cut = |v: f64, lo: f64, hi: f64| if lo > hi { 0.0 } else { v.clamp(lo.min(0.0), hi.max(0.0)) };
        let (gain_db, delay_ms) = (cut(gain_db, gain_lo, gain_hi), cut(delay_ms, delay_lo, delay_hi));

        let mut amp_ids = Vec::new();
        for amp in &mut self.amp_assignments {
            let mut touched = false;
            for channel in amp.channels.iter_mut().filter(|c| member(c)) {
                channel.output_volume_db += gain_db;
                channel.delay_out_ms += delay_ms;
                if let Some(muted) = mute {
                    channel.output_muted = muted;
                }
                touched = true;
            }
            if touched {
                amp_ids.push(amp.id.clone());
            }
        }
        Shifted { gain_db, delay_ms, amp_ids }
    }

    /// Removes a project speaker. The outputs it was linked to keep their
    /// values and their library reference, as speakers set up on the amp —
    /// including what its group added to them; take it out of the group
    /// first to give that back.
    pub fn remove_speaker(&mut self, speaker_id: &str) {
        self.speakers.retain(|s| s.id != speaker_id);
        for group in &mut self.groups {
            group.speaker_ids.retain(|id| id != speaker_id);
        }
        for channel in self.amp_assignments.iter_mut().flat_map(|a| &mut a.channels) {
            if let Some(speaker) = &mut channel.speaker {
                if speaker.project_speaker_id.as_deref() == Some(speaker_id) {
                    speaker.project_speaker_id = None;
                }
            }
        }
    }
}

impl AmpAssignment {
    pub fn new(
        mac: Option<String>,
        device_name: Option<String>,
        channel_count: u32,
        amp_model_id: Option<String>,
        firmware_version: Option<String>,
    ) -> Self {
        let channels = (0..channel_count).map(new_channel).collect();
        Self {
            id: new_id(),
            mac,
            amp_model_id,
            firmware_version,
            channels,
            device_name,
            live_disengaged: false,
        }
    }

    /// Grows/shrinks `channels` to match `channel_count`, preserving existing
    /// per-channel config where indices still exist — never a destructive
    /// wipe, per the amp-model-reconciliation rule in the data model plan.
    pub fn reconcile_channel_count(&mut self, channel_count: u32) {
        if self.channels.len() as u32 > channel_count {
            self.channels.truncate(channel_count as usize);
        } else {
            for channel_index in self.channels.len() as u32..channel_count {
                self.channels.push(new_channel(channel_index));
            }
        }
    }

    /// Grows/shrinks every channel's `matrix_crosspoints` to
    /// `matrix_input_count`, preserving existing crosspoint values at
    /// surviving indices — same non-destructive rule as
    /// `reconcile_channel_count`. Called whenever the assigned model (hence
    /// its resolved topology) changes. A newly added crosspoint follows the
    /// amp's natural 1:1 routing: In N → Out N active at 0 dB, the rest off.
    pub fn reconcile_matrix_size(&mut self, matrix_input_count: u32) {
        for channel in &mut self.channels {
            let channel_index = channel.channel_index;
            if channel.matrix_crosspoints.len() as u32 > matrix_input_count {
                channel.matrix_crosspoints.truncate(matrix_input_count as usize);
            } else {
                for source_index in channel.matrix_crosspoints.len() as u32..matrix_input_count {
                    channel.matrix_crosspoints.push(MatrixCrosspoint {
                        source_index,
                        gain_db: 0.0,
                        active: source_index == channel_index,
                    });
                }
            }
        }
    }

    /// Grows/shrinks every channel's `input_eq.bands`/`output_eq.bands` to
    /// `eq_bands_per_channel - 2` (2 slots reserved for hp/lp), preserving
    /// existing band values at surviving indices — same non-destructive rule
    /// as `reconcile_matrix_size`. For CVR this is a no-op today
    /// (`eq_bands_per_channel` is a constant 10), but stays correct the
    /// moment a differently-shaped model/protocol shows up.
    pub fn reconcile_eq_bands(&mut self, eq_bands_per_channel: u32) {
        let target = eq_bands_per_channel.saturating_sub(2);
        for channel in &mut self.channels {
            reconcile_bands(&mut channel.input_eq.bands, target);
            reconcile_bands(&mut channel.output_eq.bands, target);
        }
    }
}

fn new_channel(channel_index: u32) -> AmpChannel {
    AmpChannel {
        channel_index,
        ohms: 8.0,
        // Every built-in CVR model has one analog input per channel
        // (`capability::cvr::builtin_topology`), so Analog N is always in range.
        source: ChannelSource { kind: SourceKind::Analog, index: channel_index },
        matrix_crosspoints: Vec::new(),
        delay_in_ms: 0.0,
        input_muted: false,
        output_trim_db: 0.0,
        output_volume_db: 0.0,
        delay_out_ms: 0.0,
        input_eq: default_channel_eq(),
        output_eq: default_channel_eq(),
        limiter: default_limiter(),
        noise_gate_enabled: false,
        noise_gate_threshold_dbu: 0.0,
        output_phase_inverted: false,
        input_name: None,
        output_name: None,
        output_muted: false,
        output_bridged: false,
        power_mode: default_power_mode(),
        fir_bypassed: false,
        source_trims: SourceTrims::default(),
        backup_priority: BackupPriority::default(),
        speaker: None,
        fir: ChannelFir::identity(),
    }
}

fn default_power_mode() -> PowerMode {
    PowerMode::LowOhm
}

/// Both stages disabled by default, with in-range starting values so the
/// Limiter sub-tab never opens showing an out-of-range number.
fn default_limiter() -> Limiter {
    Limiter {
        rms: RmsLimiter {
            enabled: false,
            threshold_vrms: 100.0,
            attack_ms: 5.0,
            release_multiplier: 4.0,
            auto: false,
            max_vrms: 0.0,
        },
        peak: PeakLimiter { enabled: false, threshold_vp: 140.0, hold_ms: 10.0, release_ms: 50.0, max_vp: 0.0 },
    }
}

/// One octave apart, 100Hz.. — an arbitrary but reasonable starting spread
/// across the audible range for the 8 parametric bands. Index beyond the
/// table (only possible on a hypothetical non-CVR model with >8 parametric
/// bands) falls back to 1kHz.
const DEFAULT_BAND_FREQS_HZ: [f64; 8] = [100.0, 200.0, 400.0, 800.0, 1600.0, 3200.0, 6400.0, 12800.0];

fn default_eq_band(index: u32) -> EqBand {
    EqBand {
        filter_type: EqFilterType::Peaking,
        freq_hz: DEFAULT_BAND_FREQS_HZ.get(index as usize).copied().unwrap_or(1000.0),
        gain_db: 0.0,
        q: 1.0,
        active: false,
    }
}

/// Grows/shrinks a parametric-band `Vec` to `target` length, preserving
/// existing values at surviving indices — the shared worker behind
/// `AmpAssignment::reconcile_eq_bands`.
fn reconcile_bands(bands: &mut Vec<EqBand>, target: u32) {
    if bands.len() as u32 > target {
        bands.truncate(target as usize);
    } else {
        for index in bands.len() as u32..target {
            bands.push(default_eq_band(index));
        }
    }
}

/// Flat-response default for a freshly-created channel's EQ chain (both
/// `new_channel` and the `#[serde(default = ...)]` backfill for old project
/// files use this): HP/LP bypassed (`active: false`) at Butterworth-12
/// (Q implied by filter type — see `CrossoverSlot`), all 8 parametric bands
/// at 0dB/inactive.
fn default_channel_eq() -> ChannelEq {
    ChannelEq {
        hp: CrossoverSlot { filter_type: CrossoverFilterType::Butterworth12, freq_hz: 20.0, active: false },
        bands: (0..8).map(default_eq_band).collect(),
        lp: CrossoverSlot { filter_type: CrossoverFilterType::Butterworth12, freq_hz: 20000.0, active: false },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shifting_a_group_moves_every_member_output_by_the_same_cut_offset() {
        let mut project = Project::new("p".into(), String::new());
        let mut amp = AmpAssignment::new(None, None, 3, None, None);
        for (channel, owner) in amp.channels.iter_mut().zip(["a", "b", "x"]) {
            channel.speaker = Some(SpeakerRef {
                library_id: "lib".into(),
                way_index: 0,
                revision: 1,
                label: String::new(),
                project_speaker_id: Some(owner.into()),
            });
        }
        amp.channels[0].output_volume_db = 4.0;
        amp.channels[1].delay_out_ms = 2.0;
        project.amp_assignments.push(amp);
        let limits = |_: &AmpAssignment| {
            Some(OutputLimits {
                volume_db: ParamRange { min: -80.0, max: 6.0 },
                delay_out_ms: ParamRange { min: 0.0, max: 20.0 },
            })
        };
        let members = vec!["a".to_string(), "b".to_string()];

        // Output A has only 2 dB of headroom, output B only 18 ms of delay left.
        let shifted = project.shift_speakers(&members, 5.0, 30.0, Some(true), limits);
        assert_eq!((shifted.gain_db, shifted.delay_ms), (2.0, 18.0));
        let ch = &project.amp_assignments[0].channels;
        assert_eq!((ch[0].output_volume_db, ch[1].output_volume_db, ch[2].output_volume_db), (6.0, 2.0, 0.0));
        assert_eq!((ch[0].delay_out_ms, ch[1].delay_out_ms), (18.0, 20.0));
        assert!(ch[0].output_muted && ch[1].output_muted && !ch[2].output_muted, "only members");

        project.shift_speakers(&members, -shifted.gain_db, -shifted.delay_ms, Some(false), limits);
        let ch = &project.amp_assignments[0].channels;
        assert_eq!((ch[0].output_volume_db, ch[1].delay_out_ms, ch[0].delay_out_ms), (4.0, 2.0, 0.0));
        assert!(!ch[0].output_muted);
    }

    #[test]
    fn removing_a_project_speaker_leaves_its_outputs_as_amp_level_speakers() {
        let mut project = Project::new("p".into(), String::new());
        let speaker = ProjectSpeaker::new("Main L".into(), "lib".into(), "Brand Model".into(), 1);
        let other = ProjectSpeaker::new("Main R".into(), "lib".into(), "Brand Model".into(), 1);
        // One way on each of two amps, plus the other speaker's way.
        for (way_index, owner) in [(0, &speaker), (1, &speaker)] {
            let mut amp = AmpAssignment::new(None, None, 2, None, None);
            let reference = |way_index, id: &str| SpeakerRef {
                library_id: "lib".into(),
                way_index,
                revision: 1,
                label: "Brand Model".into(),
                project_speaker_id: Some(id.into()),
            };
            amp.channels[0].speaker = Some(reference(way_index, &owner.id));
            amp.channels[1].speaker = Some(reference(0, &other.id));
            project.amp_assignments.push(amp);
        }
        project.speakers = vec![speaker.clone(), other.clone()];

        project.remove_speaker(&speaker.id);

        assert_eq!(project.speakers.len(), 1);
        for amp in &project.amp_assignments {
            let kept = amp.channels[0].speaker.as_ref().expect("the library reference stays");
            assert_eq!(kept.project_speaker_id, None);
            assert_eq!(amp.channels[1].speaker.as_ref().unwrap().project_speaker_id.as_deref(), Some(other.id.as_str()));
        }
    }
}
