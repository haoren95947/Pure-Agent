//! The key router: which context answers a key, and what it does.
//!
//! One binding table, in the old shell's shape: a context name, a key,
//! and the action it dispatches. Modal contexts (palette, dialog,
//! picker, settings) grab priority over the shell, and the shell's own
//! bindings answer only while no modal is open. The router is a pure
//! function of the key, the modifiers and the active context, so the
//! tests drive it without a window; the shell supplies the context and
//! carries the actions out.

/// A key context: who answers the keys right now. Modal contexts sit
/// above the shell; the shell's bindings answer while none of them is
/// open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Context {
    /// The shell itself: no modal, no focused text field.
    #[default]
    Shell,
    /// The find bar's query holds the keys.
    Find,
    /// The command palette is open.
    Palette,
    /// A dialog is open.
    Dialog,
    /// A picker is open.
    Picker,
    /// The settings window is open.
    Settings,
    /// A text field has the keys.
    Input,
}

impl Context {
    /// Whether a modal context is open above the shell.
    #[must_use]
    pub fn is_modal(self) -> bool {
        matches!(
            self,
            Self::Find | Self::Palette | Self::Dialog | Self::Picker | Self::Settings
        )
    }

    /// The context's name, as the binding table spells it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Shell => "Shell",
            Self::Find => "Find",
            Self::Palette => "Palette",
            Self::Dialog => "Dialog",
            Self::Picker => "Picker",
            Self::Settings => "Settings",
            Self::Input => "Input",
        }
    }

    /// The context `name` names, or `None` for an unknown one.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Shell" => Self::Shell,
            "Find" => Self::Find,
            "Palette" => Self::Palette,
            "Dialog" => Self::Dialog,
            "Picker" => Self::Picker,
            "Settings" => Self::Settings,
            "Input" => Self::Input,
            _ => return None,
        })
    }
}

/// The modifiers a binding needs, as egui reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    /// Whether ctrl (or cmd on macOS) is held.
    pub ctrl: bool,
    /// Whether shift is held.
    pub shift: bool,
    /// Whether alt is held.
    pub alt: bool,
}

/// What a key press carries, reduced to what a binding matches on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Press {
    /// The key's name: a character, or egui's `Key` name.
    pub key: &'static str,
    /// The modifiers held.
    pub mods: Mods,
    /// Whether the key press repeats while held.
    pub repeat: bool,
}

/// The shell's own actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Quit the app.
    Quit,
    /// Open the welcome pane for a new session.
    NewSession,
    /// Toggle the sidebar.
    ToggleSidebar,
    /// Toggle the workbench.
    ToggleWorkbench,
    /// Send the composer's prompt.
    SendPrompt,
    /// Open find over the transcript.
    OpenFind,
    /// Step the find bar to its next match.
    FindNext,
    /// Step the find bar to its previous match.
    FindPrev,
    /// Close the find bar.
    FindClose,
    /// Open the command palette.
    OpenPalette,
    /// Open the settings window.
    OpenSettings,
    /// Open the providers section of settings.
    OpenProviders,
    /// Toggle swarm mode.
    ToggleSwarm,
    /// Toggle plan mode.
    TogglePlan,
    /// Step the permission mode to the next one the agent offers.
    CycleMode,
    /// Open the goal dialog.
    SetGoal,
    /// Close the open dialog.
    CloseDialog,
    /// Show the agents pane in the workbench.
    ShowAgents,
    /// Show the changes pane in the workbench.
    ReviewChanges,
}

/// One binding: the context it answers in, the key, the modifiers and
/// the action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    /// The context the binding answers in.
    pub context: Context,
    /// The key's name, as egui spells it.
    pub key: &'static str,
    /// The modifiers the binding needs.
    pub mods: Mods,
    /// The action the router dispatches.
    pub action: Action,
}

/// The `ctrl` (or `cmd`) modifier.
const CTRL: Mods = Mods {
    ctrl: true,
    shift: false,
    alt: false,
};

/// The shell's bindings, in the old table's order. The find bar and
/// the palette answer in their own contexts, so Enter, the arrows and
/// Esc act only while their query holds the keys.
#[must_use]
pub fn bindings() -> Vec<Binding> {
    use Action::*;
    vec![
        Binding {
            context: Context::Shell,
            key: "Q",
            mods: CTRL,
            action: Quit,
        },
        Binding {
            context: Context::Shell,
            key: "N",
            mods: CTRL,
            action: NewSession,
        },
        Binding {
            context: Context::Shell,
            key: "B",
            mods: CTRL,
            action: ToggleWorkbench,
        },
        Binding {
            context: Context::Shell,
            key: "\\",
            mods: CTRL,
            action: ToggleSidebar,
        },
        Binding {
            context: Context::Shell,
            key: "Enter",
            mods: CTRL,
            action: SendPrompt,
        },
        Binding {
            context: Context::Shell,
            key: "F",
            mods: CTRL,
            action: OpenFind,
        },
        Binding {
            context: Context::Shell,
            key: "K",
            mods: CTRL,
            action: OpenPalette,
        },
        Binding {
            context: Context::Shell,
            key: ",",
            mods: CTRL,
            action: OpenSettings,
        },
        Binding {
            context: Context::Find,
            key: "Enter",
            mods: Mods::default(),
            action: FindNext,
        },
        Binding {
            context: Context::Find,
            key: "Enter",
            mods: Mods {
                ctrl: false,
                shift: true,
                alt: false,
            },
            action: FindPrev,
        },
        Binding {
            context: Context::Find,
            key: "Escape",
            mods: Mods::default(),
            action: FindClose,
        },
        Binding {
            context: Context::Dialog,
            key: "Escape",
            mods: Mods::default(),
            action: CloseDialog,
        },
        Binding {
            context: Context::Picker,
            key: "ArrowUp",
            mods: Mods::default(),
            action: OpenProviders,
        },
        Binding {
            context: Context::Picker,
            key: "ArrowDown",
            mods: Mods::default(),
            action: OpenProviders,
        },
        Binding {
            context: Context::Picker,
            key: "Enter",
            mods: Mods::default(),
            action: OpenProviders,
        },
        Binding {
            context: Context::Picker,
            key: "Escape",
            mods: Mods::default(),
            action: OpenProviders,
        },
        Binding {
            context: Context::Settings,
            key: "Escape",
            mods: Mods::default(),
            action: CloseDialog,
        },
        // Inside the input's own context, so it wins over the toolkit's
        // outdent binding while the composer is focused.
        Binding {
            context: Context::Input,
            key: "Tab",
            mods: Mods {
                ctrl: false,
                shift: true,
                alt: false,
            },
            action: CycleMode,
        },
    ]
}

/// The action `press` dispatches while `context` answers, or `None`
/// when no binding matches. A modal context answers only its own
/// bindings, and the shell's bindings answer only while no modal is
/// open, which the table's `context` field already encodes.
#[must_use]
pub fn dispatch(context: Context, press: &Press) -> Option<Action> {
    bindings()
        .into_iter()
        .find(|binding| {
            binding.context == context && binding.key == press.key && binding.mods == press.mods
        })
        .map(|binding| binding.action)
}

#[cfg(test)]
mod tests {
    use super::{Action, Binding, Context, Mods, Press, bindings, dispatch};

    fn press(key: &'static str, mods: Mods) -> Press {
        Press {
            key,
            mods,
            repeat: false,
        }
    }

    fn ctrl(key: &'static str) -> Press {
        press(
            key,
            Mods {
                ctrl: true,
                shift: false,
                alt: false,
            },
        )
    }

    #[test]
    fn the_shell_bindings_match_the_old_table() {
        let expected: &[(&str, Action)] = &[
            ("Q", Action::Quit),
            ("N", Action::NewSession),
            ("B", Action::ToggleWorkbench),
            ("\\", Action::ToggleSidebar),
            ("Enter", Action::SendPrompt),
            ("F", Action::OpenFind),
            ("K", Action::OpenPalette),
            (",", Action::OpenSettings),
        ];
        for (key, action) in expected {
            let found = dispatch(Context::Shell, &ctrl(key));
            assert_eq!(found, Some(*action), "ctrl-{key}");
        }
    }

    #[test]
    fn a_modal_context_answers_only_its_own_bindings() {
        // The palette closes itself on Escape, so no shell action runs
        // for it, and the shell never sees the key.
        let escape = press(
            "Escape",
            Mods {
                ctrl: false,
                shift: false,
                alt: false,
            },
        );
        assert_eq!(dispatch(Context::Palette, &escape), None);
        assert_eq!(dispatch(Context::Shell, &escape), None, "no shell escape");
    }

    #[test]
    fn the_find_context_owns_enter_and_escape() {
        let enter = press(
            "Enter",
            Mods {
                ctrl: false,
                shift: false,
                alt: false,
            },
        );
        let shift_enter = press(
            "Enter",
            Mods {
                ctrl: false,
                shift: true,
                alt: false,
            },
        );
        assert_eq!(dispatch(Context::Find, &enter), Some(Action::FindNext));
        assert_eq!(
            dispatch(Context::Find, &shift_enter),
            Some(Action::FindPrev)
        );
        // The shell's ctrl-enter still sends while find is open? No:
        // the modal wins.
        assert_eq!(dispatch(Context::Find, &ctrl("Enter")), None);
    }

    #[test]
    fn the_input_context_owns_shift_tab() {
        let shift_tab = press(
            "Tab",
            Mods {
                ctrl: false,
                shift: true,
                alt: false,
            },
        );
        assert_eq!(
            dispatch(Context::Input, &shift_tab),
            Some(Action::CycleMode)
        );
        assert_eq!(dispatch(Context::Shell, &shift_tab), None);
    }

    #[test]
    fn every_context_name_round_trips() {
        for context in [
            Context::Shell,
            Context::Find,
            Context::Palette,
            Context::Dialog,
            Context::Picker,
            Context::Settings,
            Context::Input,
        ] {
            assert_eq!(Context::from_name(context.name()), Some(context));
        }
    }

    #[test]
    fn every_binding_names_a_known_context() {
        for binding in bindings() {
            assert_eq!(
                Context::from_name(binding.context.name()),
                Some(binding.context)
            );
        }
    }

    #[test]
    fn no_binding_answers_twice_in_one_context() {
        let mut seen: Vec<Binding> = Vec::new();
        for binding in bindings() {
            let clash = seen.iter().any(|other| {
                binding.context == other.context
                    && binding.key == other.key
                    && binding.mods == other.mods
            });
            assert!(!clash, "{binding:?} answers twice");
            seen.push(binding);
        }
    }

}
