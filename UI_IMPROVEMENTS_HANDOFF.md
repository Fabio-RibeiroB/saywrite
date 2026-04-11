# UI Improvements Handoff Plan

## Current Status
- Working in worktree: `/home/fabio/Documents/GitHub/saywrite/.claude/worktrees/ui-improvements`
- Branch: `worktree-ui-improvements`
- All UI files have been read and analyzed
- 8 tasks created in task list (see task IDs #1-#8 below)
- No code changes yet implemented

## Project Context
**SayWrite** is a Linux GTK4/libadwaita dictation app. The goal is to implement 10 UX improvements identified in the exploration.

### Key Files to Modify
- `src/ui/main_window.rs` — Main dictation window (spinner, transcript, buttons)
- `src/ui/onboarding.rs` — Welcome carousel flow (mic test, engine selection)
- `src/ui/preferences.rs` — Settings dialog
- `resources/style.css` — CSS styling
- `src/config.rs` — Settings structure
- `src/host_api.rs` — Constants for insertion modes
- `src/host_integration.rs` — Host companion interaction

## Implementation Tasks (Priority Order)

### Task #1: Add Waveform Bars for Listening State
**File:** `src/ui/main_window.rs`  
**What:** Replace the spinner with 5 animated waveform bars during listening

**Implementation Details:**
1. Create a new struct `WaveformBox` that contains 5 `gtk::DrawingArea` widgets
2. In `MainWindowUi`, replace `spinner` with `waveform_box`
3. For each bar, animate height using `glib::timeout_add_local` with sine wave at 90ms intervals
4. Bar heights should scale from 12-48px based on a repeating sine function
5. In `apply_toggle_success(starting=true)`, show waveform and hide spinner
6. Add CSS class `.waveform-bar` to each bar (already defined in style.css at line 72-75 with gradient)

**Code pattern:**
```rust
let heights = [1.0, 0.7, 0.4, 0.7, 1.0]; // relative heights
let animation_offset = Rc::new(RefCell::new(0.0));
// Animate each bar with phase offset
```

---

### Task #2: Add Smooth State Transitions with GtkRevealer
**File:** `src/ui/main_window.rs`  
**What:** Wrap spinner/waveform, setup panel, transcript, and action row in GtkRevealers for fade transitions

**Implementation Details:**
1. Wrap each of these in `gtk::Revealer`:
   - `spinner_row` (and new waveform_box)
   - `setup_panel`
   - `transcript_bubble`
   - `action_row`
2. Set `reveal_child` property instead of `set_visible()`
3. Each revealer should have transition_type `CROSSFADE` and transition_duration `300ms`
4. Update all `.set_visible()` calls on these elements to `.set_reveal_child()`
5. Add CSS to `.transcript-bubble` (line 31-38 in style.css) — already has `transition: opacity 200ms ease`

**Code pattern:**
```rust
let revealer = gtk::Revealer::new();
revealer.set_reveal_child(false);
revealer.set_transition_type(gtk::RevealerTransitionType::Crossfade);
revealer.set_transition_duration(300);
revealer.set_child(Some(&content));
```

---

### Task #3: Add Inline Setup Resolution Actions
**File:** `src/ui/main_window.rs`  
**What:** Replace generic "Open Settings" button with context-specific actions (Download Model, Set API Key, etc.)

**Implementation Details:**
1. Modify `set_setup_state()` to accept an optional action type enum:
   ```rust
   enum SetupAction {
       None,
       DownloadModel(ModelSize),
       EnterApiKey,
       OpenSettings,
   }
   ```
2. In the setup panel, replace the single `setup_action` button with a Box that can hold either:
   - "Download Model" button → triggers model_installer download
   - "Set API Key" button → shows a simple text entry field inline
   - "Open Settings" button → fallback
3. Update check_readiness logic (lines 384-428) to populate the setup_action

**Triggers for each:**
- No whisper.cpp + Local mode → "Download Model" (but whisper.cpp is bundled; check if it's really missing)
- No local model + Local mode → "Download Model" button
- No API key + Cloud mode → "Set API Key" inline field
- No host companion → "Open Settings" (can't resolve inline safely)

---

### Task #4: Add Insertion Mode Indicator to Header
**File:** `src/ui/main_window.rs`, `src/ui/main_window.rs` CSS  
**What:** Show clipboard/typing mode chip in header bar next to engine mode chip

**Implementation Details:**
1. In `build_header()`, after `mode_chip`, add a second chip `insertion_chip`
2. Use `host_integration::host_status()` to determine insertion capability
3. Display icon + label based on `insertion_capability`:
   - `"typing"` → keyboard icon + "Direct Typing"
   - `"clipboard-only"` → clipboard icon + "Clipboard"
   - `"notification-only"` → bell icon + "Notification"
   - `None` (no host) → network icon + "Offline"
4. Add CSS class `.insertion-chip` (similar to `.mode-chip`)
5. Update on host status changes via D-Bus signal

**Code pattern:**
```rust
let insertion_chip = gtk::Button::new();
insertion_chip.add_css_class("flat");
insertion_chip.add_css_class("insertion-chip");
let icon_name = match status.insertion_capability {
    "typing" => "input-keyboard-symbolic",
    "clipboard-only" => "edit-copy-symbolic",
    ...
};
```

---

### Task #5: Improve Transcript Interaction
**Files:** `src/ui/main_window.rs`, `resources/style.css`  
**What:** Add retry button, word count, and make transcript editable

**Implementation Details:**
1. Replace `transcript_label` (read-only Label) with `gtk::TextBuffer` + `gtk::TextView`
   - Set editable, word wrappable, selectable
   - Set monospace/default font
2. Wrap TextView in the existing transcript_bubble
3. Add a secondary row below transcript with:
   - Word count label on left (update on text change)
   - Character count label in center
   - "Retry" button on right
4. The "Retry" button should:
   - Hide transcript and action row
   - Show state_label "Ready"
   - Allow starting new dictation without needing to dismiss first
5. Connect `buffer.connect_changed()` to update counts

**CSS considerations:**
- TextView needs styling to match current transcript-text (line 40-44)
- Add `.transcript-textview` class for proper padding/margins

---

### Task #6: Add Cancel Button for Model Downloads in Onboarding
**File:** `src/ui/onboarding.rs`  
**What:** Allow canceling model download on engine selection page

**Implementation Details:**
1. In `engine_page()`, add a new `cancel_btn` below the progress bar
2. The button should be visible only when download is in progress
3. When clicked:
   - Stop the download thread (you'll need to use `std::sync::atomic::AtomicBool` flag in the spawn)
   - Call `model_installer::cleanup_partial()` or `cleanup_partial_for_size()`
   - Reset button/progress visibility
4. Also show estimated time remaining based on download speed
5. Add text like "Cancel" button that's only visible during download

**Code pattern:**
```rust
let cancel_requested = Rc::new(AtomicBool::new(false));
let cancel_flag_clone = cancel_requested.clone();
thread::spawn(move || {
    // Check cancel_flag_clone.load() periodically in the download loop
});
```

---

### Task #7: Add Recording Indicator to Mic Test
**File:** `src/ui/onboarding.rs`  
**What:** Show visual feedback (progress bar or pulsating bar) during mic test recording

**Implementation Details:**
1. In `mic_page()`, add a `gtk::ProgressBar` above or below `status_label`
2. When "Test Microphone" is clicked:
   - Show the progress bar
   - Call `.pulse()` in a timeout loop every 100ms for 1.5 seconds (the test duration)
   - When test completes, hide the bar
3. Add label below bar like "Recording…" that updates to "Done!" on completion

**Code pattern:**
```rust
let progress = gtk::ProgressBar::new();
progress.set_visible(false);
let timeout_id = glib::timeout_add_local(Duration::from_millis(100), {
    move || {
        progress.pulse();
        glib::ControlFlow::Continue
    }
});
```

---

### Task #8: Add Settings Save Confirmation Toast
**File:** `src/ui/preferences.rs`  
**What:** Show brief toast notification after settings are saved via debounce

**Implementation Details:**
1. In `schedule_settings_save()` (line 535), after the save completes:
   - Show a toast via GTK's built-in notification system or a custom revealer toast
2. The simplest approach: add a toast label in the preferences window that fades in/out
3. After 300ms debounce + 100ms for actual save, show toast for 2 seconds then hide
4. Toast message: "Settings saved"

**Code pattern:**
```rust
// In the timeout callback after settings.save()
let toast_label = gtk::Label::new(Some("Settings saved"));
toast_label.add_css_class("notification-toast");
// Show for 2 seconds then hide
glib::timeout_add_local(Duration::from_secs(2), move || {
    toast_label.set_visible(false);
    glib::ControlFlow::Break
});
```

---

## Additional Improvements (Lower Priority, Can Skip)

### Not Implemented (Requires More Complex Changes):
- **Waveform responds to actual audio levels** — requires passing mic input data from capture thread to UI, very complex
- **Shortcut re-binding in app** — requires GTK event capture and potentially dconf/GSettings integration
- **Engine cards show download size** — simple text addition to onboarding.rs line 274-281

---

## Testing Checklist
- [ ] Waveform animates smoothly during listening
- [ ] State transitions fade in/out (not snap)
- [ ] Setup panel actions work inline (model download, API key entry)
- [ ] Header shows both engine mode and insertion mode chips
- [ ] Transcript can be edited and shows word/char count
- [ ] Retry button works and allows new dictation
- [ ] Mic test shows progress bar during recording
- [ ] Model download can be canceled
- [ ] Settings save shows brief toast confirmation
- [ ] All CSS transitions render smoothly
- [ ] App compiles without warnings

## Next Agent Instructions
1. Start with **Task #1** (waveform bars) — it's visual and self-contained
2. Then **Task #2** (smooth transitions) — wraps existing elements
3. Then work through #3-#8 in order
4. Test after each task with `cargo build` and visual inspection
5. Commit each task when complete
6. Update task status as you go
7. After all 8 tasks, build and test the full flow

## Build & Run
```bash
cd /home/fabio/Documents/GitHub/saywrite/.claude/worktrees/ui-improvements
cargo build
cargo run
```

## File Locations Summary
- UI code: `src/ui/*.rs`
- CSS: `resources/style.css`
- Config: `src/config.rs`
- Models & host: `src/model_installer.rs`, `src/host_integration.rs`
- Constants: `src/host_api.rs`

Good luck! This is a solid UX improvement set.
