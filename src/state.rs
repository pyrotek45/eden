// Eden DAW — Application state
// Holds everything the app needs: project, UI state, mode, view params.

use std::collections::HashSet;

use crate::commands::CommandManager;
use crate::models::*;
use crate::theme::Theme;

use serde::{Deserialize, Serialize};

/// Stereo waveform peak envelope: (left_max, left_min, right_max, right_min).
pub type StereoWaveformPeaks = (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>);

/// Get the user's home directory, falling back to current dir.
fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Grid snap resolutions, as beats per division.
/// 1/4 = 1 beat, 1/8 = 0.5 beats, etc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapSettings {
    pub enabled: bool,
    /// Index into SNAP_RESOLUTIONS
    pub resolution_idx: usize,
}

/// Available snap resolutions: (display label, beats per division)
pub const SNAP_RESOLUTIONS: &[(&str, f64)] = &[
    ("1/1", 4.0),
    ("1/2", 2.0),
    ("1/4", 1.0),
    ("1/8", 0.5),
    ("1/16", 0.25),
    ("1/32", 0.125),
];

impl SnapSettings {
    pub fn resolution_beats(&self) -> f64 {
        SNAP_RESOLUTIONS[self.resolution_idx].1
    }

    /// Hard snap — rounds to nearest grid line (for loop ruler / handles).
    pub fn snap(&self, beats: f64) -> f64 {
        if !self.enabled {
            return beats;
        }
        let r = self.resolution_beats();
        (beats / r).round() * r
    }

    /// Proximity snap — only snaps if within `threshold_beats` of a grid line,
    /// otherwise returns `beats` unchanged.  Use this for clip body drag so the
    /// clip moves freely and only "clicks" onto grid lines when close enough.
    pub fn snap_proximity(&self, beats: f64, threshold_beats: f64) -> f64 {
        if !self.enabled {
            return beats;
        }
        let r = self.resolution_beats();
        let nearest = (beats / r).round() * r;
        if (beats - nearest).abs() <= threshold_beats {
            nearest
        } else {
            beats
        }
    }
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            resolution_idx: 2,
        } // default: 1/4 note snap
    }
}

/// Which main view is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppMode {
    ProjectManager,
    Arrangement,
    Mixer,
    Edit, // context-sensitive: piano roll for MIDI, waveform for audio, automation editor for automation
}

/// The active UI layer — determines which layer owns input this frame.
/// Higher variants completely shadow all layers below them.
/// Evaluated once per frame; background layers receive a dead (no-op) InputState.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UiLayer {
    /// Normal arrangement / mixer / edit view — all panels share input normally.
    Base,
    /// Project settings or Options panel is open.
    Popup,
    /// Render/export dialog is open.
    RenderDialog,
    /// A blocking confirmation dialog (delete track, delete clip, etc.) is open.
    ConfirmDialog,
}

/// Identifies who opened the generic file browser popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBrowserCaller {
    /// Home screen "Open Project" button
    OpenProject,
    /// Audio export popup — choosing export directory
    AudioExportDir,
    /// MIDI export popup — choosing export directory
    MidiExportDir,
    /// Render/Export popup — choosing export directory
    RenderExportDir,
}

/// Which tab is active in the bottom panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BottomPanelTab {
    Mixer,
    PianoRoll,
    InstrumentRack,
    MasterRack,
}

/// Which tab is active in the left side panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftPanelTab {
    Files,
    Clips,
    Instruments,
    Themes,
}

/// A node in the sample browser tree.
/// Can be a folder (with children) or a file (leaf).
#[derive(Debug, Clone)]
pub struct SampleTreeNode {
    /// Display name (file/folder basename)
    pub name: String,
    /// Full path on disk
    pub path: std::path::PathBuf,
    /// Is this a directory?
    pub is_dir: bool,
    /// Is this folder expanded in the UI?
    pub expanded: bool,
    /// Children (only populated for directories)
    pub children: Vec<SampleTreeNode>,
}

const AUDIO_EXTENSIONS: &[&str] = &["wav", "flac", "ogg", "mp3", "aiff", "aif", "mid", "midi"];

impl SampleTreeNode {
    /// Recursively scan a directory and build a tree of folders + audio files.
    /// `depth` limits recursion to avoid scanning huge trees.
    pub fn scan_dir(path: &std::path::Path, depth: usize) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        let mut children = Vec::new();

        if depth < 8 {
            if let Ok(entries) = std::fs::read_dir(path) {
                let mut items: Vec<std::path::PathBuf> =
                    entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
                items.sort();

                // Folders first, then files
                let mut dirs: Vec<std::path::PathBuf> = Vec::new();
                let mut files: Vec<std::path::PathBuf> = Vec::new();

                for item in items {
                    let fname = item
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if fname.starts_with('.') {
                        continue; // skip hidden
                    }
                    if item.is_dir() {
                        dirs.push(item);
                    } else if let Some(ext) = item.extension().and_then(|e| e.to_str()) {
                        if AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                            files.push(item);
                        }
                    }
                }

                for d in dirs {
                    let child = SampleTreeNode::scan_dir(&d, depth + 1);
                    // Only include directories that contain audio files (directly or nested)
                    if !child.children.is_empty() {
                        children.push(child);
                    }
                }

                for f in files {
                    let fname = f
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    children.push(SampleTreeNode {
                        name: fname,
                        path: f,
                        is_dir: false,
                        expanded: false,
                        children: Vec::new(),
                    });
                }
            }
        }

        SampleTreeNode {
            name,
            path: path.to_path_buf(),
            is_dir: true,
            expanded: depth == 0, // only root starts expanded; sub-folders start collapsed
            children,
        }
    }
}

/// Which UI panel currently has keyboard/mouse focus.
/// Controls where Delete, arrow keys, etc. are dispatched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedPanel {
    #[default]
    Arrangement,
    PianoRoll,
    AutomationEditor,
    AudioEditor,
}

/// Per-frame audio level data pushed from main thread (derived from AudioShared).
#[derive(Debug, Clone, Default)]
pub struct MeterState {
    /// RMS level per track (0.0–1.0), updated each frame.
    pub track_rms: Vec<f32>,
    /// Pre-effect RMS per track — used for compressor GR meter display.
    pub track_rms_pre_effect: Vec<f32>,
    /// Oscilloscope sample ring buffer (recent ~512 samples).
    pub oscilloscope: Vec<f32>,
    /// Peak level per track (decay slowly).
    pub track_peak: Vec<f32>,
    /// Peak-hold level per track (slow decay for visual peak indicator).
    pub track_peak_hold: Vec<f32>,
    /// Clipping flag per track — set when RMS >= 1.0, cleared on click.
    pub track_clipping: Vec<bool>,
    /// Master bus RMS level (post-effect, pre-volume).
    pub master_rms: f32,
    /// Master bus pre-effect RMS level.
    pub master_rms_pre: f32,
    /// Gain reduction in dB for each effect slot per track: track_idx → Vec<f32> per slot.
    pub track_effect_gr: Vec<Vec<f32>>,
    /// Gain reduction in dB for each master rack effect slot.
    pub master_effect_gr: Vec<f32>,
    // ── Stereo metering (L/R per track) ──
    /// Per-track left-channel RMS (0.0–1.0).
    pub track_rms_l: Vec<f32>,
    /// Per-track right-channel RMS (0.0–1.0).
    pub track_rms_r: Vec<f32>,
    /// Per-track left-channel peak hold (slow decay).
    pub track_peak_hold_l: Vec<f32>,
    /// Per-track right-channel peak hold (slow decay).
    pub track_peak_hold_r: Vec<f32>,
    /// Per-track left-channel clip flag.
    pub track_clipping_l: Vec<bool>,
    /// Per-track right-channel clip flag.
    pub track_clipping_r: Vec<bool>,
    // ── VU ballistic state (GUI-side, per track) ──
    /// VU needle position per track (0.0–1.0, with ballistic smoothing).
    pub vu_needle: Vec<f32>,
    // ── Master stereo metering ──
    pub master_rms_l: f32,
    pub master_rms_r: f32,
    pub master_peak_l: f32,
    pub master_peak_r: f32,
    pub master_peak_hold_l: f32,
    pub master_peak_hold_r: f32,
    pub master_lufs_short: f32,
    pub master_lufs_momentary: f32,
    pub master_clipping_l: bool,
    pub master_clipping_r: bool,
}

/// Viewport / scroll state for the arrangement timeline.
#[derive(Debug, Clone)]
pub struct ArrangementView {
    pub scroll_x: f64, // in beats
    pub scroll_y: i32, // in pixels
    pub zoom_x: f64,   // pixels per beat
    pub zoom_y: f64,   // vertical scale
    pub track_header_width: i32,
}

impl Default for ArrangementView {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0,
            zoom_x: 40.0, // 40px per beat
            zoom_y: 1.0,
            track_header_width: 180,
        }
    }
}

/// Full application state. NOT serialized (project is serialized separately).
pub struct AppState {
    pub project: Project,
    pub commands: CommandManager,
    pub theme: Theme,
    pub mode: AppMode,
    pub arrangement: ArrangementView,
    pub selected_track: Option<u32>,
    pub selected_tracks: std::collections::HashSet<u32>,
    pub selected_clip: Option<(u32, usize)>, // (track_id, clip_index)
    pub window_width: u32,
    pub window_height: u32,
    pub running: bool,
    pub last_save_path: Option<String>,
    pub dirty: bool,
    /// Whether user config needs to be saved (UI settings changed)
    pub config_dirty: bool,
    /// Frame counter for periodic config auto-save (save after N frames of dirty)
    pub config_save_countdown: u32,
    /// Autosave: enabled flag (mirrors config)
    pub autosave_enabled: bool,
    /// Autosave: interval index into AUTOSAVE_INTERVALS
    pub autosave_interval_idx: usize,
    /// Autosave: frame counter (decrements each frame, saves when reaching 0)
    pub autosave_countdown: u64,
    pub status_message: Option<String>,
    /// Frames remaining to display the status toast
    pub status_timer: u32,
    pub snap: SnapSettings,
    /// Which dropdown widget is currently open (0 = none). Shared across all dropdowns.
    pub dropdown_open_id: u32,
    /// Multi-selected clips: set of (track_id, clip_index)
    pub selected_clips: HashSet<(u32, usize)>,
    /// Rubberband selection rect: (x1, y1, x2, y2) in screen pixels, while dragging
    pub rubberband: Option<(i32, i32, i32, i32)>,
    /// Snapshot of selected_clips taken at rubber-band start (used for shift+rubber-band append)
    pub rubberband_pre_selection: HashSet<(u32, usize)>,
    /// Clipboard: list of (source_track_id, clip). Populated on Ctrl+C / Ctrl+D.
    pub clipboard: Vec<(u32, Clip)>,
    // ── Bottom panel ─────────────────────────────────────────────────
    /// Current height of the bottom panel in pixels (min = handle_h, max = ~window/2)
    pub bottom_panel_height: i32,
    /// Is the bottom panel expanded (above handle-only size)?
    pub bottom_panel_open: bool,
    /// Active tab in the bottom panel
    pub bottom_panel_tab: BottomPanelTab,
    /// Whether the user is currently dragging the bottom panel resize handle
    pub bottom_panel_dragging: bool,
    /// Remembered click type from mouse-press, held until mouse-release fires
    pub bottom_panel_click_type: Option<crate::input::ClickType>,
    // ── Audio metering ───────────────────────────────────────────────
    pub meters: MeterState,
    /// Master output volume (UI-side, synced to audio thread each frame)
    pub master_volume_ui: f32,
    // ── UI Scaling ───────────────────────────────────────────────────
    /// Global UI scale factor (1.0 = 100%, 1.5 = 150%, 2.0 = 200%)
    pub ui_scale: f32,
    /// Pending UI scale — set by slider/presets, applied when Apply is clicked
    pub ui_scale_pending: f32,
    /// Font (pixel-label) scale: 1 = small, 2 = normal, 3 = large
    pub font_scale: i32,
    /// Pending font scale — staged until Apply is clicked
    pub font_scale_pending: i32,
    /// Is the Options popup currently open?
    pub options_open: bool,
    /// Is the Project Settings popup currently open?
    pub project_popup_open: bool,
    /// Is the Render/Export popup currently open?
    pub render_popup_open: bool,
    /// Render output filename
    pub render_filename: String,
    /// Render output directory
    pub render_export_dir: String,
    /// Render sample rate index (0=44100, 1=48000, 2=96000)
    pub render_sample_rate_idx: usize,
    /// Render bit depth index (0=16, 1=24, 2=32f)
    pub render_bit_depth_idx: usize,
    /// If true, only render the loop region instead of the full arrangement
    pub render_loop_only: bool,
    /// 0.0..1.0 progress of a background render, or None if not rendering
    pub render_progress: Option<std::sync::Arc<std::sync::atomic::AtomicU32>>,
    pub render_result: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    // ── Clip drag state ──────────────────────────────────────────────
    /// Whether the current clip drag is a copy (Ctrl+drag)
    pub clip_drag_is_copy: bool,
    /// The cloned copy made at drag start for Ctrl+drag (track_id, clip_index)
    pub clip_drag_copy: Option<(u32, usize)>,
    /// Stores original start times of clips when a drag operation begins
    pub drag_original_positions: std::collections::HashMap<(u32, usize), f64>,
    /// Stores original audio offset for left-edge resize of audio clips
    pub drag_audio_offset_orig: f64,
    /// Ghost positions for clone drag visual feedback: (display_track_id, source_track_id, clip_idx, new_start)
    pub clip_drag_ghost_positions: Vec<(u32, u32, usize, f64)>,
    /// Target track for cross-track clip drag (None = same track)
    pub clip_drag_target_track: Option<u32>,
    pub clip_drag_target_valid: bool,

    // ── Audio editor → arranger drag ─────────────────────────────────
    /// Whether a Ctrl+drag from the audio editor is in progress
    pub audio_drag_to_arranger: bool,
    /// Source file path for the dragged audio region
    pub audio_drag_source: String,
    /// Offset into the source file (seconds) for the dragged region
    pub audio_drag_offset: f64,
    /// Length of the dragged region (seconds)
    pub audio_drag_length_secs: f64,

    // ── Text field state ─────────────────────────────────────────────
    /// ID of the currently focused/active text field (0 = none)
    pub text_field_active_id: u32,
    /// Current text buffer for the active text field
    pub text_field_buffer: String,
    /// Cursor position (character index) within the text field
    pub text_field_cursor: usize,

    // ── Transport settings ───────────────────────────────────────────
    /// Auto-return to start position when playback stops
    pub auto_return: bool,
    /// Position before play started (for auto-return on stop)
    pub pre_play_position: f64,
    /// True when the user has explicitly clicked to seek; cleared after pushing to audio thread
    pub seek_pending: bool,
    // ── Piano roll view ──────────────────────────────────────────────
    /// Horizontal scroll offset of piano roll (in beats)
    pub piano_roll_scroll_x: f64,
    /// Vertical scroll offset of piano roll (in semitones from top, 0 = C9 top)
    pub piano_roll_scroll_y: i32,
    /// Horizontal zoom of piano roll (pixels per beat)
    pub piano_roll_zoom_x: f64,
    /// Index of the note being dragged (move) in the piano roll, if any
    pub piano_roll_drag_note: Option<usize>,
    /// Snap grid resolution index for the piano roll (index into SNAP_RESOLUTIONS)
    pub piano_roll_snap_idx: usize,
    /// Currently selected notes in the piano roll (by index in the clip's note vec)
    pub piano_roll_selected_notes: std::collections::HashSet<usize>,
    /// Active edit tool: false = draw/erase, true = select
    pub piano_roll_select_mode: bool,
    /// While drawing a new note by drag: (start_beat, pitch, drag_start_x, drag_start_beat)
    pub piano_roll_draw_drag: Option<(f64, u8, i32, f64)>,
    /// While doing a note-move drag: original note positions (idx → (start, pitch))
    pub piano_roll_move_origins: std::collections::HashMap<usize, (f64, u8)>,
    /// While doing a note-resize drag: original note sizes (idx → (start, length))
    pub piano_roll_resize_origins: std::collections::HashMap<usize, (f64, f64)>,
    /// Is the user currently scrubbing/moving selected notes?
    pub piano_roll_moving: bool,
    /// Is the current piano roll move a clone (Ctrl+drag)?
    pub piano_roll_clone_drag: bool,
    /// Rubber-band select rect in the piano roll: (x1, y1, x2, y2) screen coords
    pub piano_roll_rubberband: Option<(i32, i32, i32, i32)>,
    /// Piano roll local playhead position (clip-relative beats, wraps within loop)
    pub piano_roll_playhead: f64,
    /// Piano roll loop length index: 0=clip length, 1=1 bar, 2=2 bars, 3=4 bars, 4=8 bars
    pub piano_roll_loop_len_idx: usize,
    /// Whether the velocity editor lane is visible in the piano roll
    pub velocity_editor_visible: bool,
    /// Whether the piano roll local playhead is playing
    pub piano_roll_playing: bool,
    // ── Clip manager ─────────────────────────────────────────────────
    /// Vertical scroll offset in the clip manager sidebar (pixels from top)
    pub clip_manager_scroll: i32,
    /// Clip being dragged from clip manager sidebar: (track_id, clip_idx)
    pub clip_sidebar_drag: Option<(u32, usize)>,
    /// A clip being dragged out of the clip library and dropped onto the arrangement.
    /// Carries the clip data so we can show a ghost and create/place it on drop.
    /// (lib_idx, clip_clone)
    pub library_drag_clip: Option<(usize, Clip)>,
    /// Library of clips that persist even when removed from the arrangement.
    /// Each entry: (original_track_id, clip). Populated when clips are created
    /// or first seen, and NOT removed when clips are deleted from tracks.
    pub clip_library: Vec<(u32, Clip)>,
    // ── Sample browser (left panel) ──────────────────────────────────
    /// Tree of loaded sample folders (multiple roots supported)
    pub sample_tree: Vec<SampleTreeNode>,
    /// Vertical scroll in the sample browser list (row units, not pixels)
    pub sample_browser_scroll: i32,
    /// When set, the browser will scroll to make this path visible on the next frame.
    pub sample_browser_scroll_to: Option<std::path::PathBuf>,
    /// Vertical scroll for themes tab (pixels)
    pub theme_scroll: i32,
    /// Vertical scroll for instruments tab (pixels)
    pub instruments_scroll: i32,
    /// Path of the sample currently being previewed
    pub sample_preview_path: Option<std::path::PathBuf>,
    /// Set to true when a new preview should be loaded and played
    pub sample_preview_trigger: bool,
    /// Start sample offset for preview playback (in output samples, 0 = from beginning)
    pub sample_preview_start_sample: usize,
    /// End sample boundary for preview playback (in output samples, 0 = play to file end)
    pub sample_preview_end_sample: usize,
    /// Preview notes to send to audio thread: Vec of (track_idx, pitch, velocity)
    pub preview_notes: Vec<(usize, u8, u8)>,
    /// Set to true when user triggers panic (stop all sounds)
    pub panic_triggered: bool,
    /// Auto-play samples when clicked in the browser
    pub sample_auto_play: bool,
    /// Whether the sample browser panel is visible
    pub sample_browser_open: bool,
    /// Width of the sample browser panel in pixels
    pub sample_browser_width: i32,
    /// If dragging a sample from the browser: the file path being dragged
    pub sample_drag_path: Option<std::path::PathBuf>,
    /// Cached clip length (in beats) for the file currently being dragged
    pub sample_drag_len_beats: Option<f64>,
    /// Favorite folder paths (persisted in user config)
    pub favorite_folders: Vec<String>,
    /// Is the in-app folder navigator open?
    pub folder_nav_open: bool,
    /// Current path being browsed in the folder navigator
    pub folder_nav_path: std::path::PathBuf,
    /// Cached directory listing for the folder navigator
    pub folder_nav_entries: Vec<(String, std::path::PathBuf, bool)>, // (name, path, is_dir)
    /// Scroll offset in the folder navigator
    pub folder_nav_scroll: i32,
    // ── Project file browser (home page "Open Project" popup) ────────
    /// Is the project file browser overlay open?
    pub project_browser_open: bool,
    /// Current path being browsed in the project file browser
    pub project_browser_path: std::path::PathBuf,
    /// Cached directory listing for the project browser (name, path, is_dir)
    pub project_browser_entries: Vec<(String, std::path::PathBuf, bool)>,
    /// Scroll offset in the project browser
    pub project_browser_scroll: i32,
    // ── Generic file browser popup (reusable) ────────────────────────
    /// Is the generic file browser overlay open?
    pub file_browser_open: bool,
    /// Who opened the file browser (determines what happens on selection)
    pub file_browser_caller: Option<FileBrowserCaller>,
    /// Current path being browsed
    pub file_browser_path: std::path::PathBuf,
    /// Cached directory listing (name, path, is_dir)
    pub file_browser_entries: Vec<(String, std::path::PathBuf, bool)>,
    /// Scroll offset
    pub file_browser_scroll: i32,
    /// Title shown at top of the popup
    pub file_browser_title: String,
    /// File extension filter (e.g. ".eden.json", ".wav", ".mid"); empty = dirs only
    pub file_browser_ext_filter: String,
    /// If true, selecting a directory is the goal (e.g. choosing export folder)
    pub file_browser_select_dir: bool,
    // ── Left panel ───────────────────────────────────────────────────
    /// Which tab is active in the left panel
    pub left_panel_tab: LeftPanelTab,
    // ── Rack UI ──────────────────────────────────────────────────────
    /// Which slot is expanded in the rack panel, if any
    pub rack_expanded_slot: Option<(u32, u32)>, // (track_id, slot_id)
    /// Horizontal scroll offset for rack view (pixels)
    pub rack_scroll_x: f32,
    /// Horizontal scroll offset for full mixer view (pixels)
    pub mixer_scroll_x: f32,
    /// Horizontal scroll offset for bottom panel mixer (pixels)
    pub bottom_mixer_scroll_x: f32,
    /// Index of rack slot being dragged for reordering (source index)
    pub rack_reorder_drag: Option<usize>,
    /// Target insertion index while dragging
    pub rack_reorder_target: Option<usize>,
    // ── Focus / Context ──────────────────────────────────────────────
    /// Which panel currently owns keyboard input (Delete, etc.)
    pub focused_panel: FocusedPanel,
    /// Paths of audio files whose samples need to be reloaded from disk
    /// (set by views.rs after destructive edits, drained by main.rs).
    pub audio_sample_invalidate: Vec<String>,
    /// Whether the add-track popup is open
    pub add_track_popup_open: bool,
    /// When true, the RACK panel shows the master output effects chain instead of a track
    pub master_rack_open: bool,
    /// Clip library: index of clip pending delete confirmation (None = no confirmation dialog)
    pub clip_lib_confirm_delete: Option<usize>,
    /// Set to true when the overlay confirmation dialog's Delete button is clicked
    pub clip_lib_confirm_execute: bool,
    /// The index to delete after confirmation
    pub clip_lib_confirmed_idx: Option<usize>,

    // ── Sidechain dropdown popup ─────────────────────────────────────
    /// Whether the sidechain picker popup list is open
    pub sc_popup_open: bool,
    /// Screen position for the sidechain popup
    pub sc_popup_x: i32,
    pub sc_popup_y: i32,
    /// Track index and slot index for the sidechain popup target
    pub sc_popup_track_idx: usize,
    pub sc_popup_slot_idx: usize,

    // ── New-project name prompt ──────────────────────────────────────
    /// When true, show a dialog prompting for the new project name
    pub new_project_popup_open: bool,
    /// Buffer for the new project name text field
    pub new_project_name_buffer: String,

    // ── Save As popup ────────────────────────────────────────────────
    /// When true, show a dialog prompting for save-as filename
    pub save_as_popup_open: bool,
    /// Buffer for the save-as filename text field
    pub save_as_name_buffer: String,
    /// Track pending delete confirmation (track_id, track_index)
    pub track_confirm_delete: Option<(u32, usize)>,
    /// Multi-track delete confirmation: list of track IDs to delete
    pub track_confirm_multi_delete: Option<Vec<u32>>,
    // ── Automation drag state ─────────────────────────────────────────
    /// Index of automation point being dragged
    pub automation_drag_idx: Option<usize>,
    /// Original time/value of automation point being dragged (for undo)
    pub automation_drag_orig: Option<(f64, f32)>,
    /// Whether snap-to-grid is enabled in the automation editor
    pub automation_snap_enabled: bool,
    /// Per-editor snap resolution index for the automation editor (into SNAP_RESOLUTIONS)
    pub automation_snap_idx: usize,
    /// Horizontal scroll (in beats) for automation editor
    pub automation_scroll_x: f64,
    /// Horizontal zoom (pixels per beat) for automation editor
    pub automation_zoom_x: f64,
    /// Selected automation point indices (for rubberband / multi-select)
    pub automation_selected: Vec<usize>,
    /// Rubberband selection rectangle start (beat, value) — None if not active
    pub automation_rubberband_start: Option<(f64, f32)>,
    /// Original positions of selected points at drag start (for group move undo)
    pub automation_group_drag_orig: Vec<(f64, f32)>,
    /// Whether the arranger auto-scrolls to follow the playhead
    pub follow_playhead: bool,
    /// Which rack parameter to highlight (track_id, slot_id, param_id)
    pub rack_highlight_param: Option<(u32, u32, String)>,
    /// Timer for rack highlight (frames remaining)
    pub rack_highlight_timer: u32,
    // ── Velocity drag state ───────────────────────────────────────────
    /// Note index being velocity-dragged
    pub drag_velocity_note_idx: Option<usize>,
    /// Original velocity before drag (for undo)
    pub drag_velocity_original: u8,
    // ── Loop region drag state ────────────────────────────────────────
    /// Original loop region start/end before drag (for undo)
    pub loop_drag_orig: Option<(f64, f64)>,
    // ── BPM drag state ───────────────────────────────────────────────
    /// Original BPM before spinner interaction (for undo)
    pub bpm_drag_orig: Option<f64>,
    pub bpm_drag_snapshot: Option<Project>,
    /// BPM text entry mode: double-clicked the spinner to type a value
    pub bpm_text_entry: bool,
    // ── Multi-track slider drag state ─────────────────────────────────
    /// When dragging volume/pan with multiple tracks selected, stores
    /// (track_id, original_value) for each selected track at drag start.
    pub multi_vol_drag_origins: Vec<(u32, f32)>,
    pub multi_pan_drag_origins: Vec<(u32, f32)>,
    /// Snapshot for multi-track volume/pan undo
    pub multi_slider_snapshot: Option<Project>,
    /// Raw mouse X at start of multi-track volume drag (pixel-accurate delta)
    pub multi_vol_drag_start_x: i32,
    pub multi_vol_slider_w: i32,
    /// Raw mouse X at start of multi-track pan drag
    pub multi_pan_drag_start_x: i32,
    // ── Piano roll snap ──────────────────────────────────────────────
    /// Independent snap toggle for the piano roll
    pub piano_roll_snap_enabled: bool,
    // ── Audio editor snap ────────────────────────────────────────────
    /// Independent snap toggle for the audio editor
    pub audio_editor_snap_enabled: bool,
    /// Per-editor snap resolution index for the audio editor (into SNAP_RESOLUTIONS)
    pub audio_editor_snap_idx: usize,
    // ── Audio editor fade ─────────────────────────────────────────────
    /// Fade-in duration in seconds (0.0 = no fade)
    pub audio_editor_fade_in: f64,
    /// Fade-out duration in seconds (0.0 = no fade)
    pub audio_editor_fade_out: f64,
    // ── Module drag state ─────────────────────────────────────────────
    /// Module name being dragged from the modules panel
    pub module_drag: Option<String>,
    /// Target insertion index when dragging a module from the panel into the rack
    pub module_drag_insert_idx: Option<usize>,
    /// Target slot index for replacing an existing module (same-category drop)
    pub module_drag_replace_idx: Option<usize>,
    // ── Audio device selection ────────────────────────────────────────
    /// List of available audio output device names
    pub audio_device_names: Vec<String>,
    /// Index of the currently selected audio device (0 = default)
    pub audio_device_idx: usize,
    /// Whether a device change has been requested
    pub audio_device_changed: bool,
    // ── Hover hint / tooltip ─────────────────────────────────────────
    /// Text to show as a hover tooltip near the cursor
    pub hover_hint: Option<String>,
    /// Screen position for the hover hint
    pub hover_hint_pos: (i32, i32),
    /// How many frames the cursor has been hovering over the same widget
    pub hover_timer: u32,
    /// Which widget was hovered last frame (for timer tracking)
    pub hover_last_widget: crate::input::WidgetId,
    // ── Waveform cache ───────────────────────────────────────────────
    /// Cache of audio waveform peak data per source file path.
    /// Key = source_file string, Value = (peaks, total_duration_seconds).
    pub waveform_cache: std::collections::HashMap<String, (Vec<f32>, f64)>,
    /// Detailed stereo waveform cache for the audio editor.
    /// Key = source_file string, Value = (left_max, left_min, right_max, right_min).
    /// Each Vec has `num_peaks` entries representing amplitude envelope over the full file.
    pub waveform_stereo_cache: std::collections::HashMap<String, StereoWaveformPeaks>,
    /// Raw stereo waveform data for high-res audio editor rendering.
    /// Key = source_file string, Value = (left_samples, right_samples, sample_rate).
    pub waveform_raw_cache: std::collections::HashMap<String, (Vec<f32>, Vec<f32>, u32)>,
    // ── Audio editor state ───────────────────────────────────────────
    /// Audio editor horizontal scroll (in samples fraction 0.0-1.0)
    pub audio_editor_scroll: f64,
    /// Audio editor horizontal zoom (pixels per sample-fraction)
    pub audio_editor_zoom: f64,
    /// Audio editor selection range: (start, end) as fractions 0.0-1.0 of the waveform
    pub audio_editor_selection: Option<(f64, f64)>,
    /// Audio editor tool: 0=select, 1=trim, 2=cut
    pub audio_editor_tool: u8,
    // ── Audio editor playhead & loop ─────────────────────────────────
    /// Audio editor independent playhead position (seconds within the audio file)
    pub audio_editor_playhead: f64,
    /// Whether the audio editor is currently playing its own preview
    pub audio_editor_playing: bool,
    /// Whether looping is enabled in the audio editor
    pub audio_editor_loop_enabled: bool,
    /// Loop region start in seconds (within the audio file)
    pub audio_editor_loop_start: f64,
    /// Loop region end in seconds (within the audio file)
    pub audio_editor_loop_end: f64,
    // ── Audio editor clipboard (for cut/paste) ───────────────────────
    /// Clipboard buffer for audio samples (mono f32, cut from source file)
    pub audio_clipboard: Option<Vec<f32>>,
    /// Sample rate of the audio clipboard samples
    pub audio_clipboard_sr: u32,
    // ── Audio editor undo stack ──────────────────────────────────────
    /// Stack of undo snapshots: (source_file_path, backup_file_path, description, project_snapshot).
    /// Each destructive operation saves a backup of the file before modifying it.
    /// The optional project snapshot restores clip metadata (offset, length) changed alongside the file.
    pub audio_undo_stack: Vec<(String, String, String, Option<crate::models::Project>)>,
    /// Redo stack (same format as undo). Cleared on new destructive operation.
    pub audio_redo_stack: Vec<(String, String, String, Option<crate::models::Project>)>,
    // ── Audio editor effects ────────────────────────────────────────
    /// Currently selected effect index in the audio editor effects dropdown
    pub audio_editor_effect_idx: usize,
    /// Recently opened project paths (newest first)
    pub recent_projects: Vec<String>,
    // ── Help screen ──────────────────────────────────────────────────
    /// Whether the help/shortcut overlay is visible
    pub help_screen_visible: bool,
    /// Currently selected tab index in the help screen
    pub help_screen_tab: usize,
    // ── Audio editor export ────────────────────────────────────────
    /// Whether the audio export popup is open
    pub audio_export_popup_open: bool,
    /// The filename for audio export
    pub audio_export_name: String,
    /// The directory path for audio export
    pub audio_export_dir: String,
    /// The source file path to export from
    pub audio_export_source: String,
    // ── MIDI export ──────────────────────────────────────────────────
    /// Whether the MIDI export popup is open
    pub midi_export_popup_open: bool,
    /// The filename for MIDI export
    pub midi_export_name: String,
    /// The directory path for MIDI export
    pub midi_export_dir: String,
    // ── Computer keyboard piano mode ─────────────────────────────────
    /// When true, QWERTY keys are mapped to MIDI notes for live playing
    pub piano_keyboard_mode: bool,
    /// Current octave offset for computer-keyboard piano (default 4)
    pub piano_keyboard_octave: i32,
    /// Set of currently held computer-keyboard piano keys (pitch values)
    pub piano_keyboard_held: std::collections::HashSet<u8>,
    /// Queue of MIDI pitches to stop (note-off) — consumed each frame by main.rs
    pub piano_note_off_queue: Vec<u8>,
    /// Pitch of the note currently being previewed by mouse-click on the piano roll keys.
    /// Tracked so we can send the correct note-off even if the mouse moves vertically.
    pub piano_roll_preview_pitch: Option<u8>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            project: Project::default(),
            commands: CommandManager::new(1000),
            theme: Theme::dark(),
            mode: AppMode::ProjectManager,
            arrangement: ArrangementView::default(),
            selected_track: None,
            selected_tracks: std::collections::HashSet::new(),
            selected_clip: None,
            window_width: 1280,
            window_height: 800,
            running: true,
            last_save_path: None,
            dirty: false,
            config_dirty: false,
            config_save_countdown: 0,
            autosave_enabled: false,
            autosave_interval_idx: 1,
            autosave_countdown: 0,
            status_message: None,
            status_timer: 0,
            snap: SnapSettings::default(),
            dropdown_open_id: 0,
            selected_clips: HashSet::new(),
            rubberband: None,
            rubberband_pre_selection: HashSet::new(),
            clipboard: Vec::new(),
            bottom_panel_height: 24, // collapsed = just the handle
            bottom_panel_open: false,
            bottom_panel_tab: BottomPanelTab::Mixer,
            bottom_panel_dragging: false,
            bottom_panel_click_type: None,
            meters: MeterState::default(),
            master_volume_ui: 0.8,
            ui_scale: 1.0,
            ui_scale_pending: 1.0,
            font_scale: 2,
            font_scale_pending: 2,
            options_open: false,
            project_popup_open: false,
            render_popup_open: false,
            render_filename: String::new(),
            render_export_dir: String::new(),
            render_sample_rate_idx: 2, // 96 kHz
            render_bit_depth_idx: 2,   // 32-bit float
            render_loop_only: false,
            render_progress: None,
            render_result: None,
            clip_drag_is_copy: false,
            clip_drag_copy: None,
            drag_original_positions: std::collections::HashMap::new(),
            drag_audio_offset_orig: 0.0,
            clip_drag_ghost_positions: Vec::new(),
            clip_drag_target_track: None,
            clip_drag_target_valid: false,
            audio_drag_to_arranger: false,
            audio_drag_source: String::new(),
            audio_drag_offset: 0.0,
            audio_drag_length_secs: 0.0,
            text_field_active_id: 0,
            text_field_buffer: String::new(),
            text_field_cursor: 0,
            auto_return: true,
            pre_play_position: 0.0,
            seek_pending: false,
            piano_roll_scroll_x: 0.0,
            piano_roll_scroll_y: 40, // center view near middle C (pitch 60 ≈ row 67 from top)
            piano_roll_zoom_x: 60.0,
            piano_roll_drag_note: None,
            piano_roll_snap_idx: 3, // default 1/8 note
            piano_roll_selected_notes: std::collections::HashSet::new(),
            piano_roll_select_mode: false,
            piano_roll_draw_drag: None,
            piano_roll_move_origins: std::collections::HashMap::new(),
            piano_roll_resize_origins: std::collections::HashMap::new(),
            piano_roll_moving: false,
            piano_roll_clone_drag: false,
            piano_roll_rubberband: None,
            piano_roll_playhead: 0.0,
            piano_roll_loop_len_idx: 0,
            velocity_editor_visible: true,
            piano_roll_playing: false,
            clip_manager_scroll: 0,
            clip_sidebar_drag: None,
            library_drag_clip: None,
            clip_library: Vec::new(),
            sample_tree: Vec::new(),
            sample_browser_scroll: 0,
            sample_browser_scroll_to: None,
            theme_scroll: 0,
            instruments_scroll: 0,
            sample_preview_path: None,
            sample_preview_trigger: false,
            sample_preview_start_sample: 0,
            sample_preview_end_sample: 0,
            preview_notes: Vec::new(),
            panic_triggered: false,
            sample_auto_play: true,
            sample_browser_open: true,
            sample_browser_width: 220,
            sample_drag_path: None,
            sample_drag_len_beats: None,
            favorite_folders: Vec::new(),
            folder_nav_open: false,
            folder_nav_path: dirs_home(),
            folder_nav_entries: Vec::new(),
            folder_nav_scroll: 0,
            project_browser_open: false,
            project_browser_path: dirs_home(),
            project_browser_entries: Vec::new(),
            project_browser_scroll: 0,
            file_browser_open: false,
            file_browser_caller: None,
            file_browser_path: dirs_home(),
            file_browser_entries: Vec::new(),
            file_browser_scroll: 0,
            file_browser_title: String::new(),
            file_browser_ext_filter: String::new(),
            file_browser_select_dir: false,
            left_panel_tab: LeftPanelTab::Files,
            rack_expanded_slot: None,
            rack_scroll_x: 0.0,
            mixer_scroll_x: 0.0,
            bottom_mixer_scroll_x: 0.0,
            rack_reorder_drag: None,
            rack_reorder_target: None,
            focused_panel: FocusedPanel::Arrangement,
            audio_sample_invalidate: Vec::new(),
            add_track_popup_open: false,
            master_rack_open: false,
            clip_lib_confirm_delete: None,
            clip_lib_confirm_execute: false,
            clip_lib_confirmed_idx: None,
            sc_popup_open: false,
            sc_popup_x: 0,
            sc_popup_y: 0,
            sc_popup_track_idx: 0,
            sc_popup_slot_idx: 0,
            new_project_popup_open: false,
            new_project_name_buffer: String::new(),
            save_as_popup_open: false,
            save_as_name_buffer: String::new(),
            track_confirm_delete: None,
            track_confirm_multi_delete: None,
            automation_drag_idx: None,
            automation_drag_orig: None,
            automation_snap_enabled: true,
            automation_snap_idx: 2, // default 1/4 note
            automation_scroll_x: 0.0,
            automation_zoom_x: 40.0,
            automation_selected: Vec::new(),
            automation_rubberband_start: None,
            automation_group_drag_orig: Vec::new(),
            follow_playhead: false,
            rack_highlight_param: None,
            rack_highlight_timer: 0,
            drag_velocity_note_idx: None,
            drag_velocity_original: 100,
            loop_drag_orig: None,
            bpm_drag_orig: None,
            bpm_drag_snapshot: None,
            bpm_text_entry: false,
            multi_vol_drag_origins: Vec::new(),
            multi_pan_drag_origins: Vec::new(),
            multi_slider_snapshot: None,
            multi_vol_drag_start_x: 0,
            multi_vol_slider_w: 1,
            multi_pan_drag_start_x: 0,
            piano_roll_snap_enabled: true,
            audio_editor_snap_enabled: true,
            audio_editor_snap_idx: 2, // default 1/4 note
            audio_editor_fade_in: 0.0,
            audio_editor_fade_out: 0.0,
            audio_device_names: Vec::new(),
            audio_device_idx: 0,
            audio_device_changed: false,
            hover_hint: None,
            hover_hint_pos: (0, 0),
            hover_timer: 0,
            hover_last_widget: crate::input::WidgetId::None,
            waveform_cache: std::collections::HashMap::new(),
            waveform_stereo_cache: std::collections::HashMap::new(),
            waveform_raw_cache: std::collections::HashMap::new(),
            audio_editor_scroll: 0.0,
            audio_editor_zoom: 1.0,
            audio_editor_selection: None,
            audio_editor_tool: 0,
            audio_editor_playhead: 0.0,
            audio_editor_playing: false,
            audio_editor_loop_enabled: false,
            audio_editor_loop_start: 0.0,
            audio_editor_loop_end: 0.0,
            audio_clipboard: None,
            audio_clipboard_sr: 44100,
            audio_undo_stack: Vec::new(),
            audio_redo_stack: Vec::new(),
            audio_editor_effect_idx: 0,
            audio_export_popup_open: false,
            audio_export_name: String::new(),
            audio_export_dir: String::new(),
            audio_export_source: String::new(),
            midi_export_popup_open: false,
            midi_export_name: String::new(),
            midi_export_dir: String::new(),
            module_drag: None,
            module_drag_insert_idx: None,
            module_drag_replace_idx: None,
            recent_projects: Vec::new(),
            help_screen_visible: false,
            help_screen_tab: 0,
            piano_keyboard_mode: false,
            piano_keyboard_octave: 4,
            piano_keyboard_held: std::collections::HashSet::new(),
            piano_note_off_queue: Vec::new(),
            piano_roll_preview_pitch: None,
        }
    }

    /// Push a status/notification message that will be shown as a toast for ~3 seconds.
    pub fn push_status(&mut self, msg: impl Into<String>) {
        self.status_message = Some(msg.into());
        self.status_timer = 180; // ~3 seconds at 60fps
    }

    /// Sync all current project clips into the clip library.
    /// Adds any clips not already present (by name+type match).
    /// Should be called after clip creation or project load.
    pub fn sync_clip_library(&mut self) {
        // Remove entries whose track no longer exists in the project
        let track_ids: Vec<u32> = self.project.tracks.iter().map(|t| t.id).collect();
        self.clip_library.retain(|(tid, _)| track_ids.contains(tid));

        // Add any new clips that aren't already in the library
        for track in &self.project.tracks {
            for clip in &track.clips {
                let already = self.clip_library.iter().any(|(tid, lc)| {
                    *tid == track.id
                        && lc.name() == clip.name()
                        && std::mem::discriminant(lc) == std::mem::discriminant(clip)
                });
                if !already {
                    self.clip_library.push((track.id, clip.clone()));
                }
            }
        }
    }

    /// Save project to JSON file.
    pub fn save_project(&mut self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.project)
            .map_err(|e| format!("Serialize error: {}", e))?;
        std::fs::write(path, json).map_err(|e| format!("Write error: {}", e))?;
        self.last_save_path = Some(path.to_string());
        self.push_recent_project(path.to_string());
        self.dirty = false;
        self.push_status(format!("Saved to {}", path));
        Ok(())
    }

    /// Autosave project to JSON file WITHOUT updating last_save_path or
    /// clearing the dirty flag. This prevents autosave from hijacking
    /// the user's manual save path.
    pub fn autosave_project(&mut self, path: &str) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.project)
            .map_err(|e| format!("Serialize error: {}", e))?;
        std::fs::write(path, json).map_err(|e| format!("Write error: {}", e))?;
        Ok(())
    }

    /// Load project from JSON file.
    pub fn load_project(&mut self, path: &str) -> Result<(), String> {
        let data = std::fs::read_to_string(path).map_err(|e| format!("Read error: {}", e))?;
        let project: Project =
            serde_json::from_str(&data).map_err(|e| format!("Deserialize error: {}", e))?;
        self.project = project;
        self.last_save_path = Some(path.to_string());
        self.dirty = false;
        self.selected_track = None;
        self.selected_clip = None;
        self.clip_library.clear();
        self.push_recent_project(path.to_string());
        self.push_status(format!("Loaded {}", path));
        Ok(())
    }

    /// Add a path to the recent projects list (max 10, newest first).
    pub fn push_recent_project(&mut self, path: String) {
        self.recent_projects.retain(|p| p != &path);
        self.recent_projects.insert(0, path);
        self.recent_projects.truncate(10);
    }

    /// Quick save (re-use last path or default).
    pub fn quick_save(&mut self) -> Result<(), String> {
        let path = self
            .last_save_path
            .clone()
            .unwrap_or_else(|| format!("{}.eden.json", self.project.name));
        self.save_project(&path)
    }

    /// Add a folder to the sample browser tree.
    /// Scans recursively and builds a tree of folders + audio files.
    pub fn add_sample_folder(&mut self, root: std::path::PathBuf) {
        // Don't add the same folder twice
        if self.sample_tree.iter().any(|n| n.path == root) {
            return;
        }
        let node = SampleTreeNode::scan_dir(&root, 0);
        self.sample_tree.push(node);
        self.sample_browser_scroll = 0;
        // Also persist to favorites so it survives restart
        let path_str = root.to_string_lossy().to_string();
        if !self.favorite_folders.contains(&path_str) {
            self.favorite_folders.push(path_str);
            self.save_config_now();
        }
    }

    /// Immediately persist the current user config to disk.
    pub fn save_config_now(&self) {
        use crate::config::UserConfig;
        use crate::state::LeftPanelTab;
        let cfg = UserConfig {
            theme_name: self.theme.name.clone(),
            favorite_folders: self.favorite_folders.clone(),
            auto_return: self.auto_return,
            ui_scale: self.ui_scale,
            snap_enabled: self.snap.enabled,
            snap_resolution_idx: self.snap.resolution_idx,
            sample_browser_open: self.sample_browser_open,
            sample_browser_width: self.sample_browser_width,
            bottom_panel_open: self.bottom_panel_open,
            bottom_panel_height: self.bottom_panel_height,
            velocity_editor_visible: self.velocity_editor_visible,
            window_width: self.window_width,
            window_height: self.window_height,
            left_panel_tab: match self.left_panel_tab {
                LeftPanelTab::Files => 0,
                LeftPanelTab::Clips => 1,
                LeftPanelTab::Instruments => 2,
                LeftPanelTab::Themes => 3,
            },
            sample_auto_play: self.sample_auto_play,
            audio_device_idx: self.audio_device_idx,
            recent_projects: self.recent_projects.clone(),
            follow_playhead: self.follow_playhead,
            autosave_enabled: self.autosave_enabled,
            autosave_interval_idx: self.autosave_interval_idx,
        };
        if let Err(e) = cfg.save() {
            eprintln!("[config] Save error: {}", e);
        }
    }

    /// Refresh the folder navigator entries for the current folder_nav_path.
    pub fn refresh_folder_nav(&mut self) {
        self.folder_nav_entries.clear();
        if let Ok(entries) = std::fs::read_dir(&self.folder_nav_path) {
            let mut items: Vec<(String, std::path::PathBuf, bool)> = entries
                .filter_map(|e| e.ok())
                .map(|e| {
                    let p = e.path();
                    let name = e.file_name().to_string_lossy().to_string();
                    let is_dir = p.is_dir();
                    (name, p, is_dir)
                })
                .filter(|(name, _, _)| !name.starts_with('.')) // hide hidden files
                .collect();
            // Sort: directories first, then alphabetical
            items.sort_by(|a, b| {
                b.2.cmp(&a.2)
                    .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            });
            self.folder_nav_entries = items;
        }
        self.folder_nav_scroll = 0;
    }

    /// Refresh the project file browser entries for the current project_browser_path.
    /// Shows directories and .eden.json files.
    pub fn refresh_project_browser(&mut self) {
        self.project_browser_entries.clear();
        if let Ok(entries) = std::fs::read_dir(&self.project_browser_path) {
            let mut items: Vec<(String, std::path::PathBuf, bool)> = entries
                .filter_map(|e| e.ok())
                .map(|e| {
                    let p = e.path();
                    let name = e.file_name().to_string_lossy().to_string();
                    let is_dir = p.is_dir();
                    (name, p, is_dir)
                })
                .filter(|(name, _path, is_dir)| {
                    if name.starts_with('.') {
                        return false;
                    }
                    // Show directories and .eden.json files
                    *is_dir || name.ends_with(".eden.json")
                })
                .collect();
            // Sort: directories first, then alphabetical
            items.sort_by(|a, b| {
                b.2.cmp(&a.2)
                    .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            });
            self.project_browser_entries = items;
        }
        self.project_browser_scroll = 0;
    }

    /// Open the generic file browser popup.
    pub fn open_file_browser(
        &mut self,
        caller: FileBrowserCaller,
        title: &str,
        ext_filter: &str,
        select_dir: bool,
        start_path: Option<&std::path::Path>,
    ) {
        self.file_browser_caller = Some(caller);
        self.file_browser_title = title.to_string();
        self.file_browser_ext_filter = ext_filter.to_string();
        self.file_browser_select_dir = select_dir;
        if let Some(p) = start_path {
            self.file_browser_path = p.to_path_buf();
        } else {
            self.file_browser_path = dirs_home();
        }
        self.refresh_file_browser();
        self.file_browser_open = true;
    }

    /// Refresh the generic file browser entries for the current file_browser_path.
    pub fn refresh_file_browser(&mut self) {
        self.file_browser_entries.clear();
        let ext_filter = self.file_browser_ext_filter.clone();
        let select_dir = self.file_browser_select_dir;
        if let Ok(entries) = std::fs::read_dir(&self.file_browser_path) {
            let mut items: Vec<(String, std::path::PathBuf, bool)> = entries
                .filter_map(|e| e.ok())
                .map(|e| {
                    let p = e.path();
                    let name = e.file_name().to_string_lossy().to_string();
                    let is_dir = p.is_dir();
                    (name, p, is_dir)
                })
                .filter(|(name, _path, is_dir)| {
                    if name.starts_with('.') {
                        return false;
                    }
                    if select_dir {
                        // In directory-selection mode, only show directories
                        return *is_dir;
                    }
                    // Show directories + files matching the extension filter
                    *is_dir || (ext_filter.is_empty() || name.ends_with(&ext_filter))
                })
                .collect();
            items.sort_by(|a, b| {
                b.2.cmp(&a.2)
                    .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            });
            self.file_browser_entries = items;
        }
        self.file_browser_scroll = 0;
    }

    /// Returns the highest-priority UI layer that is currently active.
    /// The layer that owns input this frame — everything below it gets a dead input.
    pub fn active_layer(&self) -> crate::state::UiLayer {
        // Confirm dialogs are highest priority
        if self.clip_lib_confirm_delete.is_some()
            || self.track_confirm_delete.is_some()
            || self.track_confirm_multi_delete.is_some()
        {
            return crate::state::UiLayer::ConfirmDialog;
        }
        if self.render_popup_open {
            return crate::state::UiLayer::RenderDialog;
        }
        if self.project_popup_open
            || self.options_open
            || self.new_project_popup_open
            || self.save_as_popup_open
        {
            return crate::state::UiLayer::Popup;
        }
        crate::state::UiLayer::Base
    }

    /// Cycle through available themes.
    pub fn next_theme(&mut self) {
        let all = Theme::all_themes();
        let current_idx = all
            .iter()
            .position(|t| t.name == self.theme.name)
            .unwrap_or(0);
        let next_idx = (current_idx + 1) % all.len();
        self.theme = all.into_iter().nth(next_idx).unwrap();
    }

    /// Set theme by name.
    pub fn set_theme_by_name(&mut self, name: &str) {
        let all = Theme::all_themes();
        if let Some(t) = all.into_iter().find(|t| t.name == name) {
            self.theme = t;
        }
    }

    /// Get the Clip enum for the currently selected clip, if any.
    pub fn selected_clip_type(&self) -> Option<&Clip> {
        if let Some((tid, ci)) = self.selected_clip {
            for track in &self.project.tracks {
                if track.id == tid {
                    return track.clips.get(ci);
                }
            }
        }
        None
    }

    // ── Layout helpers ───────────────────────────────────────────────
    // All values are in LOGICAL pixels (canvas set_scale already handles
    // the visual magnification, so no ui_scale multiplication here).

    pub fn transport_bar_height(&self) -> i32 {
        48
    }
    pub fn loop_ruler_height(&self) -> i32 {
        20
    }
    pub fn timeline_ruler_height(&self) -> i32 {
        28
    }
    pub fn mode_tab_height(&self) -> i32 {
        0
    } // mode buttons moved into transport bar

    /// Left offset where the arrangement track area begins.
    /// When sample browser is open, this is sample_browser_width; else 0.
    pub fn arrangement_left_offset(&self) -> i32 {
        if self.sample_browser_open {
            self.sample_browser_width
        } else {
            0
        }
    }

    /// Minimum bottom panel height (collapsed = handle strip only).
    pub const BOTTOM_PANEL_HANDLE_H: i32 = 22;
    pub fn bottom_panel_handle_h(&self) -> i32 {
        22
    }
    /// Maximum bottom panel height — can stretch up flush with the transport bar.
    pub fn bottom_panel_max_h(&self) -> i32 {
        (self.window_height as i32 - self.transport_bar_height())
            .max(self.bottom_panel_handle_h() + 60)
    }

    /// Effective bottom panel height (clamped).
    pub fn bottom_panel_effective_h(&self) -> i32 {
        if !self.bottom_panel_open {
            self.bottom_panel_handle_h()
        } else {
            let min_h = self.bottom_panel_handle_h() + 60;
            let max_h = self.bottom_panel_max_h();
            self.bottom_panel_height.clamp(min_h, max_h)
        }
    }

    /// Y offset where the track area begins.
    pub fn track_area_top(&self) -> i32 {
        self.transport_bar_height()
            + self.loop_ruler_height()
            + self.timeline_ruler_height()
            + self.mode_tab_height()
    }

    pub fn track_area_height(&self) -> i32 {
        (self.window_height as i32 - self.track_area_top() - self.bottom_panel_effective_h()).max(0)
    }

    /// Total height of all track content plus the add-track button area,
    /// used to compute maximum scroll range for the arrangement.
    pub fn total_tracks_content_height(&self) -> i32 {
        let tracks_h: i32 = self.project.tracks.iter().map(|t| t.height).sum();
        tracks_h + 40 // 40px extra for the add-track button row
    }

    /// Maximum scroll_y that still keeps the add-track button visible.
    /// Returns 0 if all content fits without scrolling.
    pub fn max_arrangement_scroll_y(&self) -> i32 {
        let content_h = self.total_tracks_content_height();
        let visible_h = self.track_area_height();
        (content_h - visible_h + 60).max(0) // 60px padding to always show button
    }

    /// Returns the Y coordinate where the bottom panel starts.
    pub fn bottom_panel_y(&self) -> i32 {
        self.window_height as i32 - self.bottom_panel_effective_h()
    }

    /// Load waveform peaks for any audio clips that aren't cached yet.
    /// Call this once per frame to lazily populate the cache.
    pub fn load_pending_waveforms(&mut self) {
        // Collect source files that need loading
        let mut needed: Vec<String> = Vec::new();
        let mut needed_stereo: Vec<String> = Vec::new();
        for track in &self.project.tracks {
            // Audio clip waveforms
            for clip in &track.clips {
                if let Clip::Audio(ac) = clip {
                    if !ac.source_file.is_empty() {
                        if !self.waveform_cache.contains_key(&ac.source_file)
                            && !needed.contains(&ac.source_file)
                        {
                            needed.push(ac.source_file.clone());
                        }
                        if !self.waveform_stereo_cache.contains_key(&ac.source_file)
                            && !needed_stereo.contains(&ac.source_file)
                        {
                            needed_stereo.push(ac.source_file.clone());
                        }
                    }
                }
            }
            // Sampler instrument waveforms
            if let Some(ref sf) = track.sampler_file {
                if !sf.is_empty() && !self.waveform_cache.contains_key(sf) && !needed.contains(sf) {
                    needed.push(sf.clone());
                }
            }
        }
        // Load one per frame to avoid stalling
        if let Some(path) = needed.first() {
            let peaks = load_waveform_peaks(path, 1024);
            self.waveform_cache.insert(path.clone(), peaks);
        }
        // Also load stereo data (one per frame)
        if let Some(path) = needed_stereo.first() {
            let (l_max, l_min, r_max, r_min) = load_waveform_stereo(path, 4096);
            self.waveform_stereo_cache
                .insert(path.clone(), (l_max, l_min, r_max, r_min));
        }
        // Load raw stereo data for audio editor high-res rendering (one file at a time)
        let ae_source: Option<String> = if let Some((track_id, clip_idx)) = self.selected_clip {
            self.project
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .and_then(|t| t.clips.get(clip_idx))
                .and_then(|c| {
                    if let Clip::Audio(ac) = c {
                        if !ac.source_file.is_empty() {
                            Some(ac.source_file.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
        } else {
            None
        };
        if let Some(sf) = ae_source {
            if let std::collections::hash_map::Entry::Vacant(e) = self.waveform_raw_cache.entry(sf)
            {
                let path_ref = std::path::Path::new(e.key());
                if let Ok((raw, channels, sr)) = crate::audio::load_audio_interleaved(path_ref) {
                    let (left, right) = if channels >= 2 {
                        let l: Vec<f32> = raw.chunks(channels).map(|ch| ch[0]).collect();
                        let r: Vec<f32> = raw
                            .chunks(channels)
                            .map(|ch| ch[1.min(channels - 1)])
                            .collect();
                        (l, r)
                    } else {
                        (raw.clone(), raw)
                    };
                    e.insert((left, right, sr));
                }
            }
        }
    }
}

/// Load waveform peak data from a WAV file.
/// Returns (peaks, total_duration_seconds).
fn load_waveform_peaks(path: &str, num_peaks: usize) -> (Vec<f32>, f64) {
    let path_ref = std::path::Path::new(path);
    let (samples, sample_rate) = match crate::audio::load_audio(path_ref) {
        Ok(v) => v,
        Err(_) => return (vec![0.0; num_peaks], 0.0),
    };
    if samples.is_empty() {
        return (vec![0.0; num_peaks], 0.0);
    }
    let total_frames = samples.len();
    let total_duration = total_frames as f64 / sample_rate as f64;
    // load_audio returns mono already; take abs for peaks
    let mono: Vec<f32> = samples.iter().map(|s| s.abs()).collect();
    // Downsample to num_peaks by taking the max of each chunk
    let chunk_size = (mono.len() / num_peaks).max(1);
    let mut peaks = Vec::with_capacity(num_peaks);
    for chunk in mono.chunks(chunk_size) {
        let peak = chunk.iter().cloned().fold(0.0f32, f32::max);
        peaks.push(peak);
    }
    // Pad if needed
    while peaks.len() < num_peaks {
        peaks.push(0.0);
    }
    (peaks, total_duration)
}

/// Load stereo waveform peak data from any supported audio file (WAV or OGG).
/// Returns (left_peaks, right_peaks) where each is a Vec of signed peak values (-1.0..1.0).
/// For mono files, both channels will be identical.
/// Returns (left_max, left_min, right_max, right_min) — all num_peaks long.
/// max = highest positive peak per chunk, min = most negative (i.e. negative value).
pub fn load_waveform_stereo(
    path: &str,
    num_peaks: usize,
) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>) {
    let zero4 = || {
        (
            vec![0.0f32; num_peaks],
            vec![0.0f32; num_peaks],
            vec![0.0f32; num_peaks],
            vec![0.0f32; num_peaks],
        )
    };
    let path_ref = std::path::Path::new(path);
    let (raw, channels, _sample_rate) = match crate::audio::load_audio_interleaved(path_ref) {
        Ok(v) => v,
        Err(_) => return zero4(),
    };
    if raw.is_empty() {
        return zero4();
    }
    // Split into left/right
    let (left, right): (Vec<f32>, Vec<f32>) = if channels >= 2 {
        let l: Vec<f32> = raw.chunks(channels).map(|ch| ch[0]).collect();
        let r: Vec<f32> = raw
            .chunks(channels)
            .map(|ch| ch[1.min(channels - 1)])
            .collect();
        (l, r)
    } else {
        (raw.clone(), raw)
    };
    // Build max (positive peak) and min (negative trough) per chunk
    fn downsample_envelope(data: &[f32], num_peaks: usize) -> (Vec<f32>, Vec<f32>) {
        let chunk_size = (data.len() / num_peaks).max(1);
        let mut maxs = Vec::with_capacity(num_peaks);
        let mut mins = Vec::with_capacity(num_peaks);
        for chunk in data.chunks(chunk_size) {
            let mx = chunk.iter().cloned().fold(0.0f32, f32::max);
            let mn = chunk.iter().cloned().fold(0.0f32, f32::min);
            maxs.push(mx);
            mins.push(mn);
        }
        while maxs.len() < num_peaks {
            maxs.push(0.0);
            mins.push(0.0);
        }
        maxs.truncate(num_peaks);
        mins.truncate(num_peaks);
        (maxs, mins)
    }
    let (l_max, l_min) = downsample_envelope(&left, num_peaks);
    let (r_max, r_min) = downsample_envelope(&right, num_peaks);
    (l_max, l_min, r_max, r_min)
}
