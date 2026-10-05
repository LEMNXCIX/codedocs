//! Global keyboard shortcuts.
//!
//! The previous implementation only handled `Ctrl+1`/`Ctrl+2` inline in the
//! layout component, so the rest of the documented shortcuts did nothing. All
//! shortcuts are declared as one table so the bindings live next to the
//! commands they invoke.

use leptos::prelude::*;

use crate::components::editor::codemirror::shortcut_for;
use crate::components::editor::EditorCommand;
use crate::state::EditorState;

/// Install the global shortcut listener.
pub fn install(state: EditorState, on_save: Callback<()>) {
    let listener =
        window_event_listener(leptos::ev::keydown, move |ev: leptos::ev::KeyboardEvent| {
            let ctrl = ev.ctrl_key() || ev.meta_key();
            if !ctrl {
                return;
            }

            for command in SHORTCUTS {
                let Some((needs_modifier, key)) = shortcut_for(*command) else {
                    continue;
                };
                if !needs_modifier || !key_matches(&ev, key) {
                    continue;
                }
                ev.prevent_default();
                command.run(state, on_save);
                return;
            }
        });

    on_cleanup(move || listener.remove());
}

/// Every command reachable from the keyboard.
const SHORTCUTS: &[EditorCommand] = &[
    EditorCommand::Save,
    EditorCommand::Bold,
    EditorCommand::Italic,
    EditorCommand::InlineCode,
    EditorCommand::Strikethrough,
    EditorCommand::Link,
    EditorCommand::Focus,
    EditorCommand::ToggleSource,
    EditorCommand::NewFile,
    EditorCommand::ExportPdf,
];

/// Match a key spec like `"b"` or `"shift+s"` against the event.
fn key_matches(ev: &leptos::ev::KeyboardEvent, spec: &str) -> bool {
    let mut wants_shift = false;
    let mut key = spec;

    if let Some(rest) = spec.strip_prefix("shift+") {
        wants_shift = true;
        key = rest;
    }

    if ev.shift_key() != wants_shift {
        return false;
    }
    ev.key().eq_ignore_ascii_case(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_has_a_shortcut() {
        // A command without a binding is unreachable from the keyboard, which
        // is how the missing shortcuts went unnoticed.
        for command in SHORTCUTS {
            assert!(
                shortcut_for(*command).is_some(),
                "{command:?} has no shortcut"
            );
        }
    }

    #[test]
    fn shortcuts_are_unique() {
        let mut seen: Vec<String> = Vec::new();
        for command in SHORTCUTS {
            // Unreachable: `shortcut_for` covers every `EditorCommand`. Kept as
            // a `filter_map` anyway so adding an unbound command cannot turn a
            // test into a panic.
            let Some((_, key)) = shortcut_for(*command) else {
                continue;
            };
            assert!(
                !seen.contains(&key.to_string()),
                "duplicate shortcut: {key}"
            );
            seen.push(key.to_string());
        }
    }

    #[test]
    fn source_toggle_is_bound_to_ctrl_slash() {
        assert_eq!(shortcut_for(EditorCommand::ToggleSource), Some((true, "/")));
    }

    #[test]
    fn the_export_is_bound_to_ctrl_p() {
        // One binding, in this table. `Mod-s` is bound *twice* — here and in
        // the CodeMirror keymap — and that is why `save_now` needs a re-entrancy
        // guard; the table's uniqueness test cannot see the second binding
        // because it lives in JavaScript. `scripts/check-export-pdf.cjs` drives
        // the real key and counts the invocations.
        assert_eq!(shortcut_for(EditorCommand::ExportPdf), Some((true, "p")));
    }
}
