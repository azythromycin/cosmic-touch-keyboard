#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellState {
    Hidden,
    CollapsedReady,
    Expanded,
}

#[derive(Debug)]
pub struct ShellStateMachine {
    pub state: ShellState,
    im_active: bool,
    user_dismissed: bool,
}

impl ShellStateMachine {
    pub fn new() -> Self {
        Self {
            state: ShellState::Hidden,
            im_active: false,
            user_dismissed: false,
        }
    }

    pub fn on_im_activate(&mut self, touch_first_only: bool) {
        self.im_active = true;
        if self.user_dismissed {
            // Keep it hidden until the compositor sends a real deactivate.
            // Some apps/tabs can emit repeated activate events while still focused.
            self.state = ShellState::Hidden;
            return;
        }
        self.state = if touch_first_only {
            // Strict touch-first mode: do not auto-show on focus events alone.
            // The user must explicitly touch the launcher area to expand.
            ShellState::Hidden
        } else {
            ShellState::Expanded
        };
    }

    pub fn on_im_deactivate(&mut self) {
        self.im_active = false;
        self.user_dismissed = false;
        self.state = ShellState::Hidden;
    }

    pub fn on_touch_expand(&mut self) {
        if self.im_active
            && !self.user_dismissed
            && matches!(self.state, ShellState::CollapsedReady | ShellState::Hidden)
        {
            self.state = ShellState::Expanded;
        }
    }

    pub fn on_user_dismiss(&mut self) {
        self.user_dismissed = true;
        self.state = ShellState::Hidden;
    }

    pub fn should_show_touch_launcher(&self) -> bool {
        self.im_active && !self.user_dismissed && self.state == ShellState::Hidden
    }
}
