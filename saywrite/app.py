from __future__ import annotations

from pathlib import Path
import traceback
import time
import threading

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")

from gi.repository import Adw, Gdk, Gio, GLib, Gtk

from .audio import MicrophoneMonitor
from .backend import probe_backends
from .cleanup import cleanup_transcript
from .config import AppSettings, load_settings, save_settings
from .hardware import detect_local_runtime
from .host_client import submit_text as submit_text_to_host
from .model_installer import default_model_path, install_default_model
from .mock_dictation import MockDictationController
from .paths import local_models_dir
from .providers import LOCAL_MODELS, CLOUD_MODELS, PROVIDER_COPY
from .transcription import run_local_transcription, transcribe_recorded_microphone


APP_ID = "io.github.fabio.SayWrite"
PROJECT_ROOT = Path(__file__).resolve().parent.parent


class InfoCard(Gtk.Box):
    def __init__(self, eyebrow: str, title: str, body: str, accent: str) -> None:
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        self.add_css_class("info-card")
        self.add_css_class(accent)

        eyebrow_label = Gtk.Label(label=eyebrow.upper(), xalign=0)
        eyebrow_label.add_css_class("eyebrow")

        title_label = Gtk.Label(label=title, wrap=True, xalign=0)
        title_label.add_css_class("card-title")

        body_label = Gtk.Label(label=body, wrap=True, xalign=0)
        body_label.add_css_class("card-body")

        self.append(eyebrow_label)
        self.append(title_label)
        self.append(body_label)


class SayWriteWindow(Adw.ApplicationWindow):
    def __init__(self, app: Adw.Application) -> None:
        super().__init__(application=app, title="SayWrite")
        self.set_default_size(1180, 820)
        self.settings = load_settings()
        self.runtime = detect_local_runtime()
        self.controller = MockDictationController(self.settings.provider_mode)
        self.monitor = MicrophoneMonitor(self._on_microphone_level, self._on_microphone_state)
        self._last_dictation_advance = 0.0
        self._transcription_running = False
        self._model_install_running = False
        self._live_transcription_running = False
        self._dictation_running = False
        self.connect("close-request", self._on_close_request)
        self.toolbar_view = Adw.ToolbarView()
        self._build_chrome()
        self._rebuild_content()

    def _build_chrome(self) -> None:
        header = Adw.HeaderBar()

        badge = Gtk.Button(label="Flatpak-First Preview")
        badge.add_css_class("pill-button")
        header.pack_start(badge)

        right_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=8)
        right_box.append(self._chip("Global Shortcut Portal"))
        right_box.append(self._chip("Local + Cloud"))
        right_box.append(self._chip("IBus Path"))
        header.pack_end(right_box)

        self.toolbar_view.add_top_bar(header)
        self.set_content(self.toolbar_view)

    def _rebuild_content(self) -> None:
        self.toolbar_view.set_content(self._build_content())

    def _build_content(self) -> Gtk.Widget:
        scroller = Gtk.ScrolledWindow(hscrollbar_policy=Gtk.PolicyType.NEVER)
        scroller.set_child(self._build_page())
        return scroller

    def _build_page(self) -> Gtk.Widget:
        outer = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=28)
        outer.set_margin_top(24)
        outer.set_margin_bottom(24)
        outer.set_margin_start(24)
        outer.set_margin_end(24)

        clamp = Adw.Clamp(maximum_size=1120, tightening_threshold=820)
        clamp.set_child(outer)

        outer.append(self._build_hero())
        outer.append(self._build_first_run())
        outer.append(self._build_microphone_section())
        outer.append(self._build_provider_section())
        outer.append(self._build_backend_section())
        outer.append(self._build_mock_dictation_section())
        outer.append(self._build_cleanup_section())
        outer.append(self._build_delivery_section())

        return clamp

    def _build_hero(self) -> Gtk.Widget:
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=18)
        box.add_css_class("hero-card")

        eyebrow = Gtk.Label(label="Simple And Clean Beats Clever And Fragile", xalign=0)
        eyebrow.add_css_class("eyebrow")

        title = Gtk.Label(
            label="Dictation for Linux that belongs in the software centre.",
            wrap=True,
            xalign=0,
        )
        title.add_css_class("hero-title")

        subtitle = Gtk.Label(
            label=(
                "SayWrite keeps setup inside the app: choose a model mode, grant microphone access, "
                "bind a shortcut through the portal, and start dictating. No external setup guide should "
                "be required for the happy path."
            ),
            wrap=True,
            xalign=0,
        )
        subtitle.add_css_class("hero-subtitle")

        actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)

        setup_button = Gtk.Button(label="Mark Onboarding Complete")
        setup_button.add_css_class("suggested-action")
        setup_button.add_css_class("cta-primary")
        setup_button.connect("clicked", self._mark_onboarding_complete)

        reset_button = Gtk.Button(label="Reset Setup")
        reset_button.add_css_class("cta-secondary")
        reset_button.connect("clicked", self._reset_settings)

        actions.append(setup_button)
        actions.append(reset_button)

        transcript_card = self._build_transcript_preview()

        box.append(eyebrow)
        box.append(title)
        box.append(subtitle)
        box.append(actions)
        box.append(transcript_card)
        return box

    def _build_first_run(self) -> Gtk.Widget:
        section = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)

        heading = Gtk.Label(label="First-Run Setup", xalign=0)
        heading.add_css_class("section-title")
        section.append(heading)

        grid = Gtk.Grid(column_spacing=16, row_spacing=16)
        grid.attach(
            InfoCard(
                "Step 1",
                "Choose local or cloud",
                "Start with a guided default instead of a backend matrix. Users can switch later without relearning the product.",
                "accent-sand",
            ),
            0,
            0,
            1,
            1,
        )
        grid.attach(
            InfoCard(
                "Step 2",
                "Bind the shortcut in-app",
                "Use the GlobalShortcuts portal where available so hotkey setup feels native to Flatpak rather than bolted on.",
                "accent-sky",
            ),
            1,
            0,
            1,
            1,
        )
        grid.attach(
            InfoCard(
                "Step 3",
                "Explain host integration clearly",
                "If deeper text integration is needed, the app should explain and drive it directly instead of sending users to a wiki.",
                "accent-rose",
            ),
            2,
            0,
            1,
            1,
        )

        section.append(grid)
        section.append(self._build_setup_panel())
        return section

    def _build_setup_panel(self) -> Gtk.Widget:
        panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        panel.add_css_class("settings-card")

        heading = Gtk.Label(label="Setup Status", xalign=0)
        heading.add_css_class("section-title")
        panel.append(heading)

        panel.append(self._status_row("Microphone", "Handled by Flatpak permission prompt on first capture."))
        panel.append(self._status_row("Shortcut", "Use portal-based global shortcut binding as the default path."))
        panel.append(self._status_row("Text Input", "Target IBus for robust cross-app insertion, with fallbacks only where needed."))
        panel.append(self._status_row("Mode", f"Current provider mode: {self.settings.provider_mode.capitalize()}."))

        summary = Gtk.Label(
            label=(
                "Onboarding complete."
                if self.settings.onboarding_complete
                else "Onboarding not completed yet. The app should guide the user through these steps, not a README."
            ),
            wrap=True,
            xalign=0,
        )
        summary.add_css_class("status-summary")
        panel.append(summary)
        return panel

    def _build_provider_section(self) -> Gtk.Widget:
        section = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)

        heading = Gtk.Label(label="Model Strategy", xalign=0)
        heading.add_css_class("section-title")
        section.append(heading)

        panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
        panel.add_css_class("settings-card")

        copy = Gtk.Label(
            label=(
                "Support both local and cloud, but make the choice feel productized. "
                "Users should see a human explanation, latency expectation, and privacy posture instead of backend jargon."
            ),
            wrap=True,
            xalign=0,
        )
        copy.add_css_class("card-body")
        panel.append(copy)

        mode_group = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=10)
        self.local_button = Gtk.ToggleButton(label="Local")
        self.cloud_button = Gtk.ToggleButton(label="Cloud")
        self.local_button.add_css_class("mode-button")
        self.cloud_button.add_css_class("mode-button")
        self.local_button.set_group(self.cloud_button)
        mode_group.append(self.local_button)
        mode_group.append(self.cloud_button)
        panel.append(mode_group)

        self.provider_copy_label = Gtk.Label(wrap=True, xalign=0)
        self.provider_copy_label.add_css_class("provider-copy")
        panel.append(self.provider_copy_label)

        self.model_list = Gtk.ListBox()
        self.model_list.add_css_class("boxed-list")
        self.model_list.set_selection_mode(Gtk.SelectionMode.NONE)
        panel.append(self.model_list)

        section.append(panel)
        self.local_button.connect("toggled", self._on_provider_toggled, "local")
        self.cloud_button.connect("toggled", self._on_provider_toggled, "cloud")
        self.local_button.set_active(self.settings.provider_mode == "local")
        self.cloud_button.set_active(self.settings.provider_mode == "cloud")
        self._refresh_provider_panel()
        return section

    def _build_backend_section(self) -> Gtk.Widget:
        section = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)

        heading = Gtk.Label(label="Backend Setup", xalign=0)
        heading.add_css_class("section-title")
        section.append(heading)

        panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        panel.add_css_class("settings-card")

        copy = Gtk.Label(
            label=(
                "Local mode should accelerate automatically: CUDA on NVIDIA, Vulkan on AMD or Intel when available, "
                "and CPU as the fallback. Cloud mode uses an API key and endpoint configured in-app."
            ),
            wrap=True,
            xalign=0,
        )
        copy.add_css_class("card-body")
        panel.append(copy)

        self.backend_summary = Gtk.Label(wrap=True, xalign=0)
        self.backend_summary.add_css_class("provider-copy")
        panel.append(self.backend_summary)

        local_model_row = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        local_model_label = Gtk.Label(label="Local model file", xalign=0)
        local_model_label.add_css_class("eyebrow")
        self.local_model_entry = Gtk.Entry()
        self.local_model_entry.set_placeholder_text(str(default_model_path()))
        self.local_model_entry.set_text(self.settings.local_model_path)
        self.local_model_entry.connect("changed", self._on_local_model_changed)
        local_model_row.append(local_model_label)
        local_model_row.append(self.local_model_entry)
        panel.append(local_model_row)

        install_copy = Gtk.Label(
            label=(
                f"Default local models live in {local_models_dir()}. "
                "The product version should drive this from the app, so the prototype does the same."
            ),
            wrap=True,
            xalign=0,
        )
        install_copy.add_css_class("provider-copy")
        panel.append(install_copy)

        cloud_base_row = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        cloud_base_label = Gtk.Label(label="Cloud API base", xalign=0)
        cloud_base_label.add_css_class("eyebrow")
        self.cloud_base_entry = Gtk.Entry()
        self.cloud_base_entry.set_text(self.settings.cloud_api_base)
        self.cloud_base_entry.connect("changed", self._on_cloud_base_changed)
        cloud_base_row.append(cloud_base_label)
        cloud_base_row.append(self.cloud_base_entry)
        panel.append(cloud_base_row)

        cloud_key_row = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        cloud_key_label = Gtk.Label(label="Cloud API key", xalign=0)
        cloud_key_label.add_css_class("eyebrow")
        self.cloud_key_entry = Gtk.PasswordEntry()
        self.cloud_key_entry.set_text(self.settings.cloud_api_key)
        self.cloud_key_entry.connect("changed", self._on_cloud_key_changed)
        cloud_key_row.append(cloud_key_label)
        cloud_key_row.append(self.cloud_key_entry)
        panel.append(cloud_key_row)

        button_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        refresh_button = Gtk.Button(label="Refresh Backend Probe")
        refresh_button.add_css_class("cta-primary")
        refresh_button.connect("clicked", self._refresh_runtime_probe)
        button_row.append(refresh_button)

        transcribe_button = Gtk.Button(label="Run Local Backend Test")
        transcribe_button.add_css_class("cta-secondary")
        transcribe_button.connect("clicked", self._run_local_backend_test)
        button_row.append(transcribe_button)

        live_transcribe_button = Gtk.Button(label="Record 5s And Transcribe")
        live_transcribe_button.add_css_class("cta-secondary")
        live_transcribe_button.connect("clicked", self._record_and_transcribe)
        button_row.append(live_transcribe_button)

        install_button = Gtk.Button(label="Install Default Local Model")
        install_button.add_css_class("cta-secondary")
        install_button.connect("clicked", self._install_default_model)
        button_row.append(install_button)

        panel.append(button_row)

        self.backend_test_output = Gtk.Label(
            label="No local backend test run yet.",
            wrap=True,
            xalign=0,
        )
        self.backend_test_output.add_css_class("provider-copy")
        self.backend_test_output.set_selectable(True)
        panel.append(self.backend_test_output)

        self.model_install_output = Gtk.Label(
            label="No local model install run yet.",
            wrap=True,
            xalign=0,
        )
        self.model_install_output.add_css_class("provider-copy")
        self.model_install_output.set_selectable(True)
        panel.append(self.model_install_output)

        self.live_transcription_output = Gtk.Label(
            label="No live microphone transcription run yet.",
            wrap=True,
            xalign=0,
        )
        self.live_transcription_output.add_css_class("provider-copy")
        self.live_transcription_output.set_selectable(True)
        panel.append(self.live_transcription_output)

        self.backend_details = Gtk.ListBox()
        self.backend_details.add_css_class("boxed-list")
        self.backend_details.set_selection_mode(Gtk.SelectionMode.NONE)
        panel.append(self.backend_details)

        section.append(panel)
        self._refresh_backend_ui()
        return section

    def _build_microphone_section(self) -> Gtk.Widget:
        section = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)

        heading = Gtk.Label(label="Microphone Access", xalign=0)
        heading.add_css_class("section-title")
        section.append(heading)

        panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        panel.add_css_class("settings-card")

        explainer = Gtk.Label(
            label=(
                "The app should request microphone access from inside the product. "
                "For the happy path, the user presses one button and the platform prompt appears."
            ),
            wrap=True,
            xalign=0,
        )
        explainer.add_css_class("card-body")
        panel.append(explainer)

        button_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)

        start_button = Gtk.Button(label="Start Microphone Check")
        start_button.add_css_class("cta-primary")
        start_button.connect("clicked", self._start_microphone_check)

        stop_button = Gtk.Button(label="Stop")
        stop_button.add_css_class("cta-secondary")
        stop_button.connect("clicked", self._stop_microphone_check)

        button_row.append(start_button)
        button_row.append(stop_button)
        panel.append(button_row)

        self.mic_status = Gtk.Label(
            label="Microphone not started yet.",
            wrap=True,
            xalign=0,
        )
        self.mic_status.add_css_class("provider-copy")

        self.mic_level_bar = Gtk.ProgressBar()
        self.mic_level_bar.set_fraction(0.0)
        self.mic_level_bar.set_hexpand(True)
        self.mic_level_bar.set_text("Input level")
        self.mic_level_bar.set_show_text(True)

        panel.append(self.mic_status)
        panel.append(self.mic_level_bar)
        section.append(panel)
        return section

    def _build_cleanup_section(self) -> Gtk.Widget:
        section = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)

        heading = Gtk.Label(label="Cleanup Quality", xalign=0)
        heading.add_css_class("section-title")
        section.append(heading)

        panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        panel.add_css_class("settings-card")

        text = "uh can you send me the latest build notes question mark"
        preview = cleanup_transcript(text)

        panel.append(
            self._status_row(
                "Product Principle",
                "Raw speech is not the final product. Cleaned text is the product, with literal mode available when needed.",
            )
        )

        raw = Gtk.Label(label=f"Raw: {text}", wrap=True, xalign=0)
        raw.add_css_class("preview-raw")
        clean = Gtk.Label(label=f"Clean: {preview}", wrap=True, xalign=0)
        clean.add_css_class("preview-clean")

        panel.append(raw)
        panel.append(clean)
        section.append(panel)
        return section

    def _build_mock_dictation_section(self) -> Gtk.Widget:
        section = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)

        heading = Gtk.Label(label="Dictation Preview", xalign=0)
        heading.add_css_class("section-title")
        section.append(heading)

        panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        panel.add_css_class("settings-card")

        copy = Gtk.Label(
            label=(
                "This section now runs the real local transcription path. Record a short dictation, then compare the "
                "raw transcript with the cleaned version that the product would commit into text fields."
            ),
            wrap=True,
            xalign=0,
        )
        copy.add_css_class("card-body")
        panel.append(copy)

        self.dictation_status = Gtk.Label(
            label="Press the button to record 5 seconds of dictation.",
            wrap=True,
            xalign=0,
        )
        self.dictation_status.add_css_class("provider-copy")

        self.dictation_raw = Gtk.Label(label="", wrap=True, xalign=0)
        self.dictation_raw.add_css_class("preview-raw")
        self.dictation_raw.set_selectable(True)

        self.dictation_clean = Gtk.Label(label="", wrap=True, xalign=0)
        self.dictation_clean.add_css_class("preview-clean")
        self.dictation_clean.set_selectable(True)

        button_row = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)

        advance = Gtk.Button(label="Record 5s Dictation")
        advance.add_css_class("cta-primary")
        advance.connect("clicked", self._run_dictation_preview)

        reset = Gtk.Button(label="Clear Preview")
        reset.add_css_class("cta-secondary")
        reset.connect("clicked", self._reset_mock_dictation)

        copy_button = Gtk.Button(label="Copy Cleaned Text")
        copy_button.add_css_class("cta-secondary")
        copy_button.connect("clicked", self._copy_cleaned_text)

        host_button = Gtk.Button(label="Type Into Focused App")
        host_button.add_css_class("cta-secondary")
        host_button.connect("clicked", self._send_cleaned_text_to_host)

        delayed_host_button = Gtk.Button(label="Type After 3s Delay")
        delayed_host_button.add_css_class("cta-secondary")
        delayed_host_button.connect("clicked", self._send_cleaned_text_to_host_with_delay)

        button_row.append(advance)
        button_row.append(reset)
        button_row.append(copy_button)
        button_row.append(host_button)
        button_row.append(delayed_host_button)

        panel.append(button_row)

        self.auto_copy_switch = Gtk.Switch(active=self.settings.auto_copy_cleaned_text, valign=Gtk.Align.CENTER)
        self.auto_copy_switch.connect("notify::active", self._on_auto_copy_toggled)
        auto_copy_row = Adw.ActionRow(
            title="Auto-copy cleaned text",
            subtitle="Copy the cleaned transcript to the clipboard after each successful dictation run.",
        )
        auto_copy_row.add_suffix(self.auto_copy_switch)
        auto_copy_row.set_activatable_widget(self.auto_copy_switch)
        panel.append(auto_copy_row)

        self.auto_type_switch = Gtk.Switch(active=self.settings.auto_type_into_focused_app, valign=Gtk.Align.CENTER)
        self.auto_type_switch.connect("notify::active", self._on_auto_type_toggled)
        auto_type_row = Adw.ActionRow(
            title="Auto-type into focused app",
            subtitle="After dictation, send the cleaned transcript to the host helper for focused-field insertion.",
        )
        auto_type_row.add_suffix(self.auto_type_switch)
        auto_type_row.set_activatable_widget(self.auto_type_switch)
        panel.append(auto_type_row)

        self.delivery_status = Gtk.Label(
            label="Focused-field delivery is idle.",
            wrap=True,
            xalign=0,
        )
        self.delivery_status.add_css_class("provider-copy")
        self.delivery_status.set_selectable(True)

        panel.append(self.dictation_status)
        panel.append(self.dictation_raw)
        panel.append(self.dictation_clean)
        panel.append(self.delivery_status)
        section.append(panel)
        return section

    def _build_delivery_section(self) -> Gtk.Widget:
        section = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)

        heading = Gtk.Label(label="Delivery Plan", xalign=0)
        heading.add_css_class("section-title")
        section.append(heading)

        panel = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        panel.add_css_class("settings-card")

        for title, subtitle in [
            ("Software Centre First", "The primary install story should be a Flatpak surfaced in GNOME Software and similar stores."),
            ("Flatpak Onboarding", "Every important setup step should be discoverable and understandable inside the app."),
            ("Host Integration", "Focused-field typing now runs through the host helper first, with clipboard retained only as a fallback when insertion is unsupported."),
        ]:
            panel.append(self._status_row(title, subtitle))

        section.append(panel)
        return section

    def _build_transcript_preview(self) -> Gtk.Widget:
        preview = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        preview.add_css_class("transcript-card")

        preview_label = Gtk.Label(label="Transcript Preview", xalign=0)
        preview_label.add_css_class("section-title")

        raw_text = "um please open bracket super alt close bracket and send the notes question mark"
        cleaned_text = cleanup_transcript(raw_text)

        raw_line = Gtk.Label(label=raw_text, xalign=0, wrap=True)
        raw_line.add_css_class("preview-raw")

        clean_line = Gtk.Label(label=cleaned_text, xalign=0, wrap=True)
        clean_line.add_css_class("preview-clean")

        preview.append(preview_label)
        preview.append(raw_line)
        preview.append(clean_line)
        return preview

    def _status_row(self, title: str, subtitle: str) -> Gtk.Widget:
        row = Adw.ActionRow(title=title, subtitle=subtitle)
        row.add_css_class("status-row")
        return row

    def _on_provider_toggled(self, button: Gtk.ToggleButton, mode: str) -> None:
        if not button.get_active():
            return

        self.settings.provider_mode = mode
        self.controller.set_provider_mode(mode)
        save_settings(self.settings)
        self._refresh_provider_panel()
        self._reset_mock_labels()

    def _refresh_provider_panel(self) -> None:
        self.provider_copy_label.set_label(PROVIDER_COPY[self.settings.provider_mode])

        child = self.model_list.get_first_child()
        while child is not None:
            next_child = child.get_next_sibling()
            self.model_list.remove(child)
            child = next_child

        models = LOCAL_MODELS if self.settings.provider_mode == "local" else CLOUD_MODELS
        for model in models:
            row = Adw.ActionRow(title=model["name"], subtitle=model["summary"])
            pill = Gtk.Label(label=model["pill"])
            pill.add_css_class("status-chip")
            row.add_suffix(pill)
            self.model_list.append(row)
        self._reset_mock_labels()

    def _refresh_backend_ui(self) -> None:
        probe = probe_backends(
            self.settings.local_model_path,
            self.settings.cloud_api_key,
            self.settings.provider_mode,
        )
        self.runtime = probe.local_runtime

        summary = (
            f"Detected GPU: {self.runtime.gpu_vendor}. "
            f"Preferred local acceleration: {self.runtime.acceleration}. "
            f"Local runtime {'found' if self.runtime.runnable else 'not found'}."
        )
        self.backend_summary.set_label(summary)

        child = self.backend_details.get_first_child()
        while child is not None:
            next_child = child.get_next_sibling()
            self.backend_details.remove(child)
            child = next_child

        detail_rows = [
            ("GPU vendor", self.runtime.gpu_vendor),
            ("Acceleration path", self.runtime.acceleration),
            ("whisper.cpp CLI", self.runtime.whisper_cli_path or "Not found on PATH"),
            ("CMake", "Installed" if self.runtime.cmake_available else "Missing"),
            ("Vulkan runtime", "Available" if self.runtime.vulkan_available else "Unavailable"),
            ("Local model file", "Configured" if probe.local_model_configured else "Not configured"),
            ("Cloud API key", "Configured" if probe.cloud_configured else "Not configured"),
        ]
        for title, subtitle in detail_rows:
            self.backend_details.append(self._status_row(title, subtitle))

    def _mark_onboarding_complete(self, _button: Gtk.Button) -> None:
        self.settings.onboarding_complete = True
        save_settings(self.settings)
        self._rebuild_content()

    def _reset_settings(self, _button: Gtk.Button) -> None:
        self.settings = AppSettings()
        self.controller.set_provider_mode(self.settings.provider_mode)
        save_settings(self.settings)
        self._rebuild_content()

    def _refresh_runtime_probe(self, _button: Gtk.Button) -> None:
        self._refresh_backend_ui()

    def _on_local_model_changed(self, entry: Gtk.Entry) -> None:
        self.settings.local_model_path = entry.get_text()
        save_settings(self.settings)
        if hasattr(self, "backend_summary"):
            self._refresh_backend_ui()

    def _on_cloud_base_changed(self, entry: Gtk.Entry) -> None:
        self.settings.cloud_api_base = entry.get_text()
        save_settings(self.settings)

    def _on_cloud_key_changed(self, entry: Gtk.PasswordEntry) -> None:
        self.settings.cloud_api_key = entry.get_text()
        save_settings(self.settings)
        if hasattr(self, "backend_summary"):
            self._refresh_backend_ui()

    def _run_local_backend_test(self, _button: Gtk.Button) -> None:
        if self._transcription_running:
            return

        self._transcription_running = True
        try:
            text = run_local_transcription(
                self.settings.local_model_path,
                self.runtime.whisper_cli_path,
            )
        except Exception as exc:
            traceback.print_exc()
            self.backend_test_output.set_label(
                f"Local backend test failed:\n{type(exc).__name__}: {exc}"
            )
        else:
            rendered = text if text else "No transcript returned."
            self.backend_test_output.set_label(f"Local backend output: {rendered}")
        finally:
            self._transcription_running = False

    def _install_default_model(self, _button: Gtk.Button) -> None:
        if self._model_install_running:
            return

        self._model_install_running = True
        self.model_install_output.set_label("Installing default local model. This may take a while.")
        thread = threading.Thread(target=self._install_default_model_worker, daemon=True)
        thread.start()

    def _install_default_model_worker(self) -> None:
        try:
            model_path = install_default_model(str(PROJECT_ROOT))
        except Exception as exc:
            traceback.print_exc()
            GLib.idle_add(
                self._finish_model_install,
                None,
                f"Local model install failed:\n{type(exc).__name__}: {exc}",
            )
            return

        GLib.idle_add(self._finish_model_install, str(model_path), f"Local model installed at {model_path}")

    def _finish_model_install(self, model_path: str | None, message: str) -> bool:
        self._model_install_running = False
        self.model_install_output.set_label(message)
        if model_path is not None:
            self.settings.local_model_path = model_path
            save_settings(self.settings)
            self.local_model_entry.set_text(model_path)
            self._refresh_backend_ui()
        return False

    def _record_and_transcribe(self, _button: Gtk.Button) -> None:
        if self._live_transcription_running:
            return

        self._live_transcription_running = True
        self.live_transcription_output.set_label(
            "Recording from the microphone for 5 seconds, then transcribing locally."
        )
        thread = threading.Thread(target=self._record_and_transcribe_worker, daemon=True)
        thread.start()

    def _record_and_transcribe_worker(self) -> None:
        try:
            text = transcribe_recorded_microphone(
                self.settings.local_model_path,
                self.runtime.whisper_cli_path,
                duration_seconds=5,
            )
        except Exception as exc:
            traceback.print_exc()
            GLib.idle_add(
                self._finish_live_transcription,
                f"Live microphone transcription failed:\n{type(exc).__name__}: {exc}",
            )
            return

        rendered = text if text else "No transcript returned."
        GLib.idle_add(self._finish_live_transcription, f"Live transcript: {rendered}")

    def _finish_live_transcription(self, message: str) -> bool:
        self._live_transcription_running = False
        self.live_transcription_output.set_label(message)
        return False

    def _run_dictation_preview(self, _button: Gtk.Button) -> None:
        if self._dictation_running:
            return

        self._dictation_running = True
        self.dictation_status.set_label("Recording dictation for 5 seconds, then transcribing locally.")
        self.dictation_raw.set_label("")
        self.dictation_clean.set_label("")
        thread = threading.Thread(target=self._run_dictation_preview_worker, daemon=True)
        thread.start()

    def _run_dictation_preview_worker(self) -> None:
        try:
            raw_text = transcribe_recorded_microphone(
                self.settings.local_model_path,
                self.runtime.whisper_cli_path,
                duration_seconds=5,
            )
        except Exception as exc:
            traceback.print_exc()
            GLib.idle_add(
                self._finish_dictation_preview,
                None,
                None,
                f"Dictation failed:\n{type(exc).__name__}: {exc}",
            )
            return

        cleaned_text = cleanup_transcript(raw_text) if raw_text else ""
        GLib.idle_add(
            self._finish_dictation_preview,
            raw_text,
            cleaned_text,
            "Dictation complete.",
        )

    def _finish_dictation_preview(self, raw_text: str | None, cleaned_text: str | None, status: str) -> bool:
        self._dictation_running = False
        self.dictation_status.set_label(status)
        self.dictation_raw.set_label(f"Raw: {raw_text}" if raw_text else "")
        self.dictation_clean.set_label(f"Cleaned: {cleaned_text}" if cleaned_text else "")
        if cleaned_text and self.settings.auto_type_into_focused_app:
            self._deliver_text_to_host(cleaned_text)
        if cleaned_text and self.settings.auto_copy_cleaned_text:
            self._copy_text_to_clipboard(cleaned_text, "Cleaned text copied to clipboard automatically.")
        return False

    def _reset_mock_dictation(self, _button: Gtk.Button) -> None:
        self._reset_mock_labels()

    def _reset_mock_labels(self) -> None:
        if hasattr(self, "dictation_status"):
            self.dictation_status.set_label("Press the button to record 5 seconds of dictation.")
        if hasattr(self, "dictation_raw"):
            self.dictation_raw.set_label("")
        if hasattr(self, "dictation_clean"):
            self.dictation_clean.set_label("")
        if hasattr(self, "delivery_status"):
            self.delivery_status.set_label("Focused-field delivery is idle.")

    def _start_microphone_check(self, _button: Gtk.Button) -> None:
        self.monitor.start()

    def _stop_microphone_check(self, _button: Gtk.Button) -> None:
        self.monitor.stop()

    def _on_microphone_state(self, text: str) -> None:
        if hasattr(self, "mic_status"):
            self.mic_status.set_label(text)

    def _on_microphone_level(self, percent: int) -> None:
        if hasattr(self, "mic_level_bar"):
            self.mic_level_bar.set_fraction(percent / 100)
            self.mic_level_bar.set_text(f"Input level {percent}%")

        if percent < 18:
            return

        now = time.monotonic()
        if now - self._last_dictation_advance < 0.9:
            return

        self._last_dictation_advance = now
        if hasattr(self, "dictation_status") and not self._dictation_running:
            self.dictation_status.set_label("Voice activity detected. Use 'Record 5s Dictation' to capture a transcript.")

    def _chip(self, text: str) -> Gtk.Widget:
        label = Gtk.Label(label=text)
        label.add_css_class("status-chip")
        return label

    def _copy_cleaned_text(self, _button: Gtk.Button) -> None:
        text = self.dictation_clean.get_label().removeprefix("Cleaned: ").strip()
        if not text:
            if hasattr(self, "delivery_status"):
                self.delivery_status.set_label("Nothing to copy yet.")
            return
        self._copy_text_to_clipboard(text, "Cleaned text copied to clipboard.")

    def _send_cleaned_text_to_host(self, _button: Gtk.Button) -> None:
        text = self.dictation_clean.get_label().removeprefix("Cleaned: ").strip()
        if not text:
            if hasattr(self, "delivery_status"):
                self.delivery_status.set_label("Nothing to send yet.")
            return
        self._deliver_text_to_host(text)

    def _send_cleaned_text_to_host_with_delay(self, _button: Gtk.Button) -> None:
        text = self.dictation_clean.get_label().removeprefix("Cleaned: ").strip()
        if not text:
            if hasattr(self, "delivery_status"):
                self.delivery_status.set_label("Nothing to send yet.")
            return
        if hasattr(self, "delivery_status"):
            self.delivery_status.set_label("Switch to the target app now. SayWrite will type in 3 seconds.")
        thread = threading.Thread(target=self._deliver_text_to_host_worker, args=(text, 3.0), daemon=True)
        thread.start()

    def _copy_text_to_clipboard(self, text: str, status: str) -> None:
        display = Gdk.Display.get_default()
        if display is None:
            if hasattr(self, "delivery_status"):
                self.delivery_status.set_label("Clipboard unavailable: no display found.")
            return
        clipboard = display.get_clipboard()
        clipboard.set(text)
        if hasattr(self, "delivery_status"):
            self.delivery_status.set_label(status)

    def _on_auto_copy_toggled(self, switch: Gtk.Switch, _param: object) -> None:
        self.settings.auto_copy_cleaned_text = switch.get_active()
        save_settings(self.settings)

    def _on_auto_type_toggled(self, switch: Gtk.Switch, _param: object) -> None:
        self.settings.auto_type_into_focused_app = switch.get_active()
        save_settings(self.settings)

    def _deliver_text_to_host(self, text: str) -> None:
        try:
            status = submit_text_to_host(text)
        except Exception as exc:
            if hasattr(self, "delivery_status"):
                self.delivery_status.set_label(f"Host helper send failed: {exc}")
            return
        if hasattr(self, "delivery_status"):
            self.delivery_status.set_label(status)

    def _deliver_text_to_host_worker(self, text: str, delay_seconds: float) -> None:
        try:
            status = submit_text_to_host(text, delay_seconds=delay_seconds)
        except Exception as exc:
            GLib.idle_add(self._set_delivery_status, f"Host helper send failed: {exc}")
            return
        GLib.idle_add(self._set_delivery_status, status)

    def _set_delivery_status(self, status: str) -> bool:
        if hasattr(self, "delivery_status"):
            self.delivery_status.set_label(status)
        return False

    def _on_close_request(self, _window: Gtk.Window) -> bool:
        self.monitor.stop()
        return False


class SayWriteApplication(Adw.Application):
    def __init__(self) -> None:
        super().__init__(application_id=APP_ID, flags=Gio.ApplicationFlags.FLAGS_NONE)
        self.connect("activate", self.on_activate)

    def on_activate(self, _app: Adw.Application) -> None:
        self._load_css()
        window = self.props.active_window
        if window is None:
            window = SayWriteWindow(self)
        window.present()

    def _load_css(self) -> None:
        provider = Gtk.CssProvider()
        provider.load_from_path(str(Path(__file__).with_name("theme.css")))
        Gtk.StyleContext.add_provider_for_display(
            Gdk.Display.get_default(),
            provider,
            Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
        )


def main() -> int:
    app = SayWriteApplication()
    return app.run([])
