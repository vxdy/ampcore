//! Speaker groups (`SpeakerGroup`): project speakers run from one control.
//!
//! A group's gain, delay and mute live on its members' outputs
//! (`Project::shift_speakers`), so every change that moves them is an output
//! change like any other: made on a copy of each amp it touches and handed to
//! `save_or_push` — pushed to a linked, online amp, saved as the plan
//! otherwise. Only then is the group's own record updated.

use tauri::{AppHandle, State};

use ampcore_core::data::amp_model::AmpModelCatalogEntry;
use ampcore_core::data::capability;
use ampcore_core::data::project::{AmpAssignment, CanvasPosition, OutputLimits, Project, Shifted, SpeakerGroup};
use ampcore_core::error::AppError;
use ampcore_core::live::state::LiveDeviceState;

use super::speakers::{edit_project, save_or_push};
use crate::data::store::ProjectDataState;

fn limits_of(models: &[AmpModelCatalogEntry], amp: &AmpAssignment) -> Option<OutputLimits> {
    let model = models.iter().find(|m| Some(&m.id) == amp.amp_model_id.as_ref())?;
    let ranges = capability::resolve(model, amp.firmware_version.as_deref()).param_ranges;
    Some(OutputLimits { volume_db: ranges.output_volume_db, delay_out_ms: ranges.delay_out_ms })
}

fn project_of(state: &State<'_, ProjectDataState>, project_id: &str) -> Result<Project, AppError> {
    let inner = state.0.lock().map_err(|e| e.to_string())?;
    inner.projects.iter().find(|p| p.id == project_id).cloned().ok_or_else(|| AppError::from("project not found"))
}

fn group_in<'a>(project: &'a Project, group_id: &str) -> Result<&'a SpeakerGroup, AppError> {
    project.groups.iter().find(|g| g.id == group_id).ok_or_else(|| AppError::from("That group is no longer in the project"))
}

/// Shifts these speakers' outputs (see `Project::shift_speakers`) and makes
/// each amp it touched the project's, one amp after the other. An amp that
/// refuses stops the rest; the ones before it keep the change.
async fn shift(
    app: &AppHandle,
    state: &State<'_, ProjectDataState>,
    live: &State<'_, LiveDeviceState>,
    project_id: &str,
    speaker_ids: &[String],
    gain_db: f64,
    delay_ms: f64,
    mute: Option<bool>,
) -> Result<Shifted, AppError> {
    if speaker_ids.is_empty() || (gain_db == 0.0 && delay_ms == 0.0 && mute.is_none()) {
        return Ok(Shifted { gain_db: 0.0, delay_ms: 0.0, amp_ids: Vec::new() });
    }
    let (shifted, candidates) = {
        let inner = state.0.lock().map_err(|e| e.to_string())?;
        let mut project =
            inner.projects.iter().find(|p| p.id == project_id).cloned().ok_or_else(|| AppError::from("project not found"))?;
        let shifted = project.shift_speakers(speaker_ids, gain_db, delay_ms, mute, |amp| limits_of(&inner.amp_models, amp));
        let candidates: Vec<AmpAssignment> =
            project.amp_assignments.into_iter().filter(|a| shifted.amp_ids.contains(&a.id)).collect();
        (shifted, candidates)
    };
    for candidate in candidates {
        let id = candidate.id.clone();
        save_or_push(app, state, live, project_id.to_string(), id, candidate, "the group's change").await?;
    }
    Ok(shifted)
}

/// Makes `speaker_ids` the group's members. Leavers get the group's gain and
/// delay taken back off their outputs (and are unmuted if it was muted);
/// joiners leave their old group the same way, then get this group's added.
async fn set_members(
    app: &AppHandle,
    state: &State<'_, ProjectDataState>,
    live: &State<'_, LiveDeviceState>,
    project_id: &str,
    group_id: &str,
    speaker_ids: Vec<String>,
) -> Result<Project, AppError> {
    let project = project_of(state, project_id)?;
    let group = group_in(&project, group_id)?.clone();
    let speaker_ids: Vec<String> =
        speaker_ids.into_iter().filter(|id| project.speakers.iter().any(|s| &s.id == id)).collect();

    let leaving: Vec<String> = group.speaker_ids.iter().filter(|id| !speaker_ids.contains(id)).cloned().collect();
    shift(app, state, live, project_id, &leaving, -group.gain_db, -group.delay_ms, group.muted.then_some(false)).await?;

    let joining: Vec<String> = speaker_ids.iter().filter(|id| !group.speaker_ids.contains(id)).cloned().collect();
    for other in project.groups.iter().filter(|g| g.id != group_id) {
        let moving: Vec<String> = other.speaker_ids.iter().filter(|id| joining.contains(id)).cloned().collect();
        shift(app, state, live, project_id, &moving, -other.gain_db, -other.delay_ms, other.muted.then_some(false)).await?;
    }
    shift(app, state, live, project_id, &joining, group.gain_db, group.delay_ms, group.muted.then_some(true)).await?;

    edit_project(app, state, project_id, |_, project| {
        for other in &mut project.groups {
            other.speaker_ids.retain(|id| !speaker_ids.contains(id));
        }
        let group = project.groups.iter_mut().find(|g| g.id == group_id).ok_or("That group is no longer in the project")?;
        group.speaker_ids = speaker_ids;
        Ok(())
    })
}

/// A new group of these speakers, at 0 dB, no delay, unmuted. Speakers in
/// another group move to it.
#[tauri::command]
#[specta::specta]
pub async fn projects_create_group(
    app: AppHandle,
    state: State<'_, ProjectDataState>,
    live: State<'_, LiveDeviceState>,
    project_id: String,
    name: String,
    speaker_ids: Vec<String>,
) -> Result<Project, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::from("A group needs a name"));
    }
    let group = SpeakerGroup::new(name.to_string());
    let group_id = group.id.clone();
    edit_project(&app, &state, &project_id, |_, project| {
        project.groups.push(group);
        Ok(())
    })?;
    set_members(&app, &state, &live, &project_id, &group_id, speaker_ids).await
}

#[tauri::command]
#[specta::specta]
pub async fn projects_set_group_members(
    app: AppHandle,
    state: State<'_, ProjectDataState>,
    live: State<'_, LiveDeviceState>,
    project_id: String,
    group_id: String,
    speaker_ids: Vec<String>,
) -> Result<Project, AppError> {
    set_members(&app, &state, &live, &project_id, &group_id, speaker_ids).await
}

/// Sets the group's gain, delay and/or mute (`None` leaves one as it is).
/// Gain and delay move every member output by the difference; when an output
/// would leave its range, all of them move only as far as it can, and the
/// group shows where they got to. The delay never goes below 0.
#[tauri::command]
#[specta::specta]
pub async fn projects_set_group_levels(
    app: AppHandle,
    state: State<'_, ProjectDataState>,
    live: State<'_, LiveDeviceState>,
    project_id: String,
    group_id: String,
    gain_db: Option<f64>,
    delay_ms: Option<f64>,
    muted: Option<bool>,
) -> Result<Project, AppError> {
    let project = project_of(&state, &project_id)?;
    let group = group_in(&project, &group_id)?.clone();
    let gain = gain_db.filter(|g| g.is_finite()).map_or(0.0, |g| g - group.gain_db);
    let delay = delay_ms.filter(|d| d.is_finite()).map_or(0.0, |d| d.max(0.0) - group.delay_ms);
    let shifted = shift(&app, &state, &live, &project_id, &group.speaker_ids, gain, delay, muted).await?;
    edit_project(&app, &state, &project_id, |_, project| {
        let group = project.groups.iter_mut().find(|g| g.id == group_id).ok_or("That group is no longer in the project")?;
        // Exactly what the outputs moved by, so leaving the group gives it back exactly.
        group.gain_db += shifted.gain_db;
        group.delay_ms += shifted.delay_ms;
        if let Some(muted) = muted {
            group.muted = muted;
        }
        Ok(())
    })
}

#[tauri::command]
#[specta::specta]
pub fn projects_rename_group(
    app: AppHandle,
    state: State<ProjectDataState>,
    project_id: String,
    group_id: String,
    name: String,
) -> Result<Project, AppError> {
    edit_project(&app, &state, &project_id, |_, project| {
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::from("A group needs a name"));
        }
        let group = project.groups.iter_mut().find(|g| g.id == group_id).ok_or("That group is no longer in the project")?;
        group.name = name.to_string();
        Ok(())
    })
}

/// Where the group's control sits on the Workspace canvas.
#[tauri::command]
#[specta::specta]
pub fn projects_set_group_position(
    app: AppHandle,
    state: State<ProjectDataState>,
    project_id: String,
    group_id: String,
    x: f64,
    y: f64,
) -> Result<Project, AppError> {
    edit_project(&app, &state, &project_id, |_, project| {
        let group = project.groups.iter_mut().find(|g| g.id == group_id).ok_or("That group is no longer in the project")?;
        group.position = Some(CanvasPosition { x, y });
        Ok(())
    })
}

/// Dissolves the group: its members get its gain and delay taken back off
/// (and are unmuted if it was muted), then it is gone. The speakers stay.
#[tauri::command]
#[specta::specta]
pub async fn projects_delete_group(
    app: AppHandle,
    state: State<'_, ProjectDataState>,
    live: State<'_, LiveDeviceState>,
    project_id: String,
    group_id: String,
) -> Result<Project, AppError> {
    set_members(&app, &state, &live, &project_id, &group_id, Vec::new()).await?;
    edit_project(&app, &state, &project_id, |_, project| {
        project.groups.retain(|g| g.id != group_id);
        Ok(())
    })
}
