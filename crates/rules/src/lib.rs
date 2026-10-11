//! Pure remap state machine for keytap.
//!
//! Assumes the macOS built-in Modifier Keys swap stays ON: physical Command
//! arrives as Control, physical Control arrives as Command. The default rule
//! rewrites Control+Tab (optional Shift) into Command+Tab so the app switcher
//! opens, and while that Control stays held after a remapped Tab, presents
//! Command as held so repeated Tab cycles and release selects.
//!
//! No I/O. The daemon feeds parsed events in and posts the returned actions.

#![forbid(unsafe_code)]

/// Hardware key identity used by rules. Values match macOS virtual keycodes
/// for the keys we care about; `Other` carries the raw code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Tab,
    ControlL,
    ControlR,
    CommandL,
    CommandR,
    ShiftL,
    ShiftR,
    Left,
    Right,
    Up,
    Down,
    Escape,
    Other(u16),
}

impl Key {
    pub fn from_code(code: u16) -> Self {
        match code {
            0x30 => Key::Tab,
            0x3B => Key::ControlL,
            0x3E => Key::ControlR,
            0x37 => Key::CommandL,
            0x36 => Key::CommandR,
            0x38 => Key::ShiftL,
            0x3C => Key::ShiftR,
            0x7B => Key::Left,
            0x7C => Key::Right,
            0x7E => Key::Up,
            0x7D => Key::Down,
            0x35 => Key::Escape,
            n => Key::Other(n),
        }
    }

    pub fn code(self) -> u16 {
        match self {
            Key::Tab => 0x30,
            Key::ControlL => 0x3B,
            Key::ControlR => 0x3E,
            Key::CommandL => 0x37,
            Key::CommandR => 0x36,
            Key::ShiftL => 0x38,
            Key::ShiftR => 0x3C,
            Key::Left => 0x7B,
            Key::Right => 0x7C,
            Key::Up => 0x7E,
            Key::Down => 0x7D,
            Key::Escape => 0x35,
            Key::Other(n) => n,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Key::Tab => "tab",
            Key::ControlL => "control_l",
            Key::ControlR => "control_r",
            Key::CommandL => "command_l",
            Key::CommandR => "command_r",
            Key::ShiftL => "shift_l",
            Key::ShiftR => "shift_r",
            Key::Left => "left",
            Key::Right => "right",
            Key::Up => "up",
            Key::Down => "down",
            Key::Escape => "escape",
            Key::Other(_) => "other",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "tab" => Key::Tab,
            "control_l" => Key::ControlL,
            "control_r" => Key::ControlR,
            "command_l" => Key::CommandL,
            "command_r" => Key::CommandR,
            "shift_l" => Key::ShiftL,
            "shift_r" => Key::ShiftR,
            "left" => Key::Left,
            "right" => Key::Right,
            "up" => Key::Up,
            "down" => Key::Down,
            "escape" => Key::Escape,
            _ => {
                if let Some(rest) = name.strip_prefix("other:") {
                    return rest.parse().ok().map(Key::Other);
                }
                return None;
            }
        })
    }

    fn is_control(self) -> bool {
        matches!(self, Key::ControlL | Key::ControlR)
    }
}

/// Modifier flags carried on an event. Subset of CGEventFlag bits we use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Flags {
    pub control: bool,
    pub command: bool,
    pub shift: bool,
    pub option: bool,
    pub caps: bool,
}

impl Flags {
    pub const fn empty() -> Self {
        Self {
            control: false,
            command: false,
            shift: false,
            option: false,
            caps: false,
        }
    }

    pub fn with_control(mut self, v: bool) -> Self {
        self.control = v;
        self
    }

    pub fn with_command(mut self, v: bool) -> Self {
        self.command = v;
        self
    }

    pub fn with_shift(mut self, v: bool) -> Self {
        self.shift = v;
        self
    }

    pub fn with_option(mut self, v: bool) -> Self {
        self.option = v;
        self
    }

    pub fn with_caps(mut self, v: bool) -> Self {
        self.caps = v;
        self
    }

    /// Rewrite Control into Command for switcher presentation.
    pub fn control_to_command(mut self) -> Self {
        if self.control {
            self.control = false;
            self.command = true;
        }
        self
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        let mut f = Flags::empty();
        let s = s.trim();
        if s.is_empty() || s == "-" {
            return Ok(f);
        }
        for part in s.split('|') {
            match part.trim() {
                "control" => f.control = true,
                "command" => f.command = true,
                "shift" => f.shift = true,
                "option" => f.option = true,
                "caps" => f.caps = true,
                other => return Err(format!("unknown flag `{other}`")),
            }
        }
        Ok(f)
    }

    pub fn render(self) -> String {
        let mut parts = Vec::new();
        if self.control {
            parts.push("control");
        }
        if self.command {
            parts.push("command");
        }
        if self.shift {
            parts.push("shift");
        }
        if self.option {
            parts.push("option");
        }
        if self.caps {
            parts.push("caps");
        }
        if parts.is_empty() {
            "-".to_string()
        } else {
            parts.join("|")
        }
    }
}

/// Kind of input from the event tap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    KeyDown,
    KeyUp,
    FlagsChanged,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::KeyDown => "key_down",
            Phase::KeyUp => "key_up",
            Phase::FlagsChanged => "flags",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "key_down" => Some(Phase::KeyDown),
            "key_up" => Some(Phase::KeyUp),
            "flags" => Some(Phase::FlagsChanged),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    pub phase: Phase,
    pub key: Key,
    pub flags: Flags,
}

impl Event {
    pub fn render(self) -> String {
        if let Key::Other(n) = self.key {
            format!("{} other:{n} {}", self.phase.name(), self.flags.render())
        } else {
            format!(
                "{} {} {}",
                self.phase.name(),
                self.key.name(),
                self.flags.render()
            )
        }
    }

    pub fn parse_line(line: &str) -> Result<Self, String> {
        let mut parts = line.split_whitespace();
        let phase = parts
            .next()
            .and_then(Phase::parse)
            .ok_or_else(|| format!("bad phase in `{line}`"))?;
        let key_s = parts
            .next()
            .ok_or_else(|| format!("missing key in `{line}`"))?;
        let key = Key::parse(key_s).ok_or_else(|| format!("bad key `{key_s}`"))?;
        let flags_s = parts.next().unwrap_or("-");
        if parts.next().is_some() {
            return Err(format!("trailing tokens in `{line}`"));
        }
        let flags = Flags::parse(flags_s)?;
        Ok(Event { phase, key, flags })
    }
}

/// What the daemon should do for one input event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Deliver the (possibly rewritten) event as-is.
    Pass(Event),
    /// Drop the input event.
    Suppress,
    /// Synthesize an extra event onto the tap stream.
    Post(Event),
}

impl Action {
    pub fn render(&self) -> String {
        match self {
            Action::Pass(e) => format!("pass {}", e.render()),
            Action::Suppress => "suppress".to_string(),
            Action::Post(e) => format!("post {}", e.render()),
        }
    }

    pub fn parse_line(line: &str) -> Result<Self, String> {
        let line = line.trim();
        if line == "suppress" {
            return Ok(Action::Suppress);
        }
        if let Some(rest) = line.strip_prefix("pass ") {
            return Ok(Action::Pass(Event::parse_line(rest)?));
        }
        if let Some(rest) = line.strip_prefix("post ") {
            return Ok(Action::Post(Event::parse_line(rest)?));
        }
        Err(format!("bad action `{line}`"))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

impl Side {
    fn from_control(key: Key) -> Option<Self> {
        match key {
            Key::ControlL => Some(Side::Left),
            Key::ControlR => Some(Side::Right),
            _ => None,
        }
    }

    fn control_key(self) -> Key {
        match self {
            Side::Left => Key::ControlL,
            Side::Right => Key::ControlR,
        }
    }

    fn command_key(self) -> Key {
        match self {
            Side::Left => Key::CommandL,
            Side::Right => Key::CommandR,
        }
    }
}

/// Which remap behaviours are enabled. Parsed from the config file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Control+Tab (and Control+Shift+Tab) -> Command+Tab with held-modifier fix.
    pub control_tab_to_command_tab: bool,
    /// Control+3 -> Command+3 (physical Command+3 under the OS swap). Only the
    /// digit 3, only with Control alone; Shift/Option combos pass through.
    pub control_3_to_command_3: bool,
    /// Control+Left/Right/Up/Down -> Command+arrow (physical Command+arrow
    /// under the OS swap: line start/end, document top/bottom). Control alone
    /// only; Shift/Option/Command combos pass through.
    pub control_arrows_to_command_arrows: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            control_tab_to_command_tab: true,
            control_3_to_command_3: true,
            control_arrows_to_command_arrows: true,
        }
    }
}

impl Config {
    /// Hand-parsed config. Lines are `key = value`. `#` comments. Unknown keys error.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut cfg = Config::default();
        for (idx, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("line {}: expected key = value", idx + 1))?;
            let k = k.trim();
            let v = v.trim();
            match k {
                "control_tab_to_command_tab" => {
                    cfg.control_tab_to_command_tab = parse_bool(v, idx + 1)?;
                }
                "control_3_to_command_3" => {
                    cfg.control_3_to_command_3 = parse_bool(v, idx + 1)?;
                }
                "control_arrows_to_command_arrows" => {
                    cfg.control_arrows_to_command_arrows = parse_bool(v, idx + 1)?;
                }
                other => return Err(format!("line {}: unknown key `{other}`", idx + 1)),
            }
        }
        Ok(cfg)
    }

    pub fn render(&self) -> String {
        format!(
            "# keytap config\ncontrol_tab_to_command_tab = {}\ncontrol_3_to_command_3 = {}\ncontrol_arrows_to_command_arrows = {}\n",
            render_bool(self.control_tab_to_command_tab),
            render_bool(self.control_3_to_command_3),
            render_bool(self.control_arrows_to_command_arrows),
        )
    }
}

fn render_bool(v: bool) -> &'static str {
    if v { "true" } else { "false" }
}

fn parse_bool(v: &str, line: usize) -> Result<bool, String> {
    match v {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        other => Err(format!("line {line}: bad bool `{other}`")),
    }
}

/// Engine state. Owned by the daemon on the heap; the tap callback gets a
/// raw pointer to it. No locks, no atomics, no lazy globals: the run loop
/// delivers events serially on one thread.
#[derive(Clone, Debug)]
pub struct Engine {
    cfg: Config,
    /// Physical Control currently down (seen as Control because of the OS swap).
    control_down: Option<Side>,
    /// After a remapped Tab: OS has been told Command is down for `side`.
    switcher: Option<Side>,
    /// Keys whose key_down was remapped Control -> Command (3, arrows); their
    /// key_up is rewritten too.
    chord_down: Vec<Key>,
}

impl Engine {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            control_down: None,
            switcher: None,
            chord_down: Vec::new(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Short label for debug logs: idle | control_down | switcher.
    pub fn debug_state(&self) -> &'static str {
        if self.switcher.is_some() {
            "switcher"
        } else if self.control_down.is_some() {
            "control_down"
        } else {
            "idle"
        }
    }

    /// Process one input event into zero or more actions, in order.
    pub fn handle(&mut self, ev: Event) -> Vec<Action> {
        if !self.cfg.control_tab_to_command_tab
            && !self.cfg.control_3_to_command_3
            && !self.cfg.control_arrows_to_command_arrows
        {
            return vec![Action::Pass(ev)];
        }
        match ev.phase {
            Phase::KeyDown => self.on_key_down(ev),
            Phase::KeyUp => self.on_key_up(ev),
            Phase::FlagsChanged => self.on_flags(ev),
        }
    }

    fn on_key_down(&mut self, ev: Event) -> Vec<Action> {
        if ev.key.is_control() {
            if self.switcher.is_some() {
                // Extra control while switcher active: suppress, keep session.
                return vec![Action::Suppress];
            }
            self.control_down = Side::from_control(ev.key);
            return vec![Action::Pass(ev)];
        }

        if self.cfg.control_tab_to_command_tab
            && ev.key == Key::Tab
            && ev.flags.control
            && !ev.flags.command
        {
            if let Some(side) = self.control_down {
                return self.begin_or_continue_switcher(side, ev);
            }
            // Control flag set but we missed the key_down (focus steal). Treat
            // the flag as enough to start a session on the left side.
            if self.switcher.is_none() {
                return self.begin_or_continue_switcher(Side::Left, ev);
            }
        }

        if let Some(side) = self.switcher {
            return self.rewrite_while_switcher(side, ev);
        }

        if self.cfg.control_3_to_command_3 && ev.key == DIGIT_3 && is_control_only(ev.flags) {
            let side = self.control_down.unwrap_or(Side::Left);
            return self.control_chord_down(side, ev);
        }

        if self.cfg.control_arrows_to_command_arrows
            && is_arrow(ev.key)
            && is_control_only(ev.flags)
        {
            let side = self.control_down.unwrap_or(Side::Left);
            return self.control_chord_down(side, ev);
        }

        vec![Action::Pass(ev)]
    }

    fn on_key_up(&mut self, ev: Event) -> Vec<Action> {
        if ev.key.is_control() {
            if let Some(side) = self.switcher
                && Side::from_control(ev.key) == Some(side)
            {
                // End switcher: suppress real Control up, release fake Command.
                self.switcher = None;
                self.control_down = None;
                let cmd = side.command_key();
                return vec![
                    Action::Suppress,
                    Action::Post(Event {
                        phase: Phase::KeyUp,
                        key: cmd,
                        flags: Flags::empty()
                            .with_shift(ev.flags.shift)
                            .with_option(ev.flags.option)
                            .with_caps(ev.flags.caps),
                    }),
                ];
            }
            if Side::from_control(ev.key) == self.control_down {
                self.control_down = None;
            }
            return vec![Action::Pass(ev)];
        }

        if let Some(side) = self.switcher {
            if ev.key == Key::Tab || is_switcher_nav(ev.key) {
                return self.rewrite_while_switcher(side, ev);
            }
            return self.rewrite_while_switcher(side, ev);
        }

        if let Some(i) = self.chord_down.iter().position(|k| *k == ev.key) {
            // Matching up for a remapped key: present it with Command, not Control.
            self.chord_down.swap_remove(i);
            let mut out = ev;
            out.flags = ev.flags.control_to_command();
            return vec![Action::Suppress, Action::Post(out)];
        }

        vec![Action::Pass(ev)]
    }

    fn on_flags(&mut self, ev: Event) -> Vec<Action> {
        if let Some(side) = self.switcher {
            // Physical Control still held produces FlagsChanged with control set.
            // Present Command instead. If control cleared, end the session.
            if !ev.flags.control {
                self.switcher = None;
                self.control_down = None;
                let mut flags = ev.flags;
                flags.command = false;
                flags.control = false;
                return vec![
                    Action::Suppress,
                    Action::Post(Event {
                        phase: Phase::KeyUp,
                        key: side.command_key(),
                        flags,
                    }),
                ];
            }
            let mut rewritten = ev;
            rewritten.flags = ev.flags.control_to_command();
            // Keep the key field as the control key the OS used for the flags
            // event; rewrite to the command key so observers see command change.
            rewritten.key = side.command_key();
            return vec![Action::Suppress, Action::Post(rewritten)];
        }

        // Track control from flags if key events were missed.
        if ev.key.is_control() {
            if ev.flags.control {
                self.control_down = Side::from_control(ev.key);
            } else if Side::from_control(ev.key) == self.control_down {
                self.control_down = None;
            }
        }

        vec![Action::Pass(ev)]
    }

    fn begin_or_continue_switcher(&mut self, side: Side, tab: Event) -> Vec<Action> {
        let shift = tab.flags.shift;
        let option = tab.flags.option;
        let caps = tab.flags.caps;
        let base = Flags::empty()
            .with_command(true)
            .with_shift(shift)
            .with_option(option)
            .with_caps(caps);

        if self.switcher.is_some() {
            // Already in session (repeat Tab). Suppress and post Command+Tab.
            return vec![
                Action::Suppress,
                Action::Post(Event {
                    phase: tab.phase,
                    key: Key::Tab,
                    flags: base,
                }),
            ];
        }

        // First Tab: retract Control, assert Command, then Tab with Command.
        self.switcher = Some(side);
        // control_down stays Some until real Control up, so we know the side.
        vec![
            Action::Suppress,
            Action::Post(Event {
                phase: Phase::KeyUp,
                key: side.control_key(),
                flags: Flags::empty()
                    .with_shift(shift)
                    .with_option(option)
                    .with_caps(caps),
            }),
            Action::Post(Event {
                phase: Phase::KeyDown,
                key: side.command_key(),
                flags: base,
            }),
            Action::Post(Event {
                phase: Phase::KeyDown,
                key: Key::Tab,
                flags: base,
            }),
        ]
    }

    /// Control+3 or Control+arrow down: drop it, retract Control, Command
    /// down, the key with Command, Command up, then put Control back down
    /// because the physical key is still held. Net modifier state after the burst equals the state before
    /// it, so nothing sticks whether Control is released next or held for
    /// another key (autorepeat runs the same burst again).
    fn control_chord_down(&mut self, side: Side, ev: Event) -> Vec<Action> {
        if !self.chord_down.contains(&ev.key) {
            self.chord_down.push(ev.key);
        }
        let rest = Flags::empty()
            .with_shift(ev.flags.shift)
            .with_option(ev.flags.option)
            .with_caps(ev.flags.caps);
        let cmd = rest.with_command(true);
        vec![
            Action::Suppress,
            Action::Post(Event {
                phase: Phase::KeyUp,
                key: side.control_key(),
                flags: rest,
            }),
            Action::Post(Event {
                phase: Phase::KeyDown,
                key: side.command_key(),
                flags: cmd,
            }),
            Action::Post(Event {
                phase: Phase::KeyDown,
                key: ev.key,
                flags: cmd,
            }),
            Action::Post(Event {
                phase: Phase::KeyUp,
                key: side.command_key(),
                flags: rest,
            }),
            Action::Post(Event {
                phase: Phase::KeyDown,
                key: side.control_key(),
                flags: rest.with_control(true),
            }),
        ]
    }

    fn rewrite_while_switcher(&mut self, _side: Side, ev: Event) -> Vec<Action> {
        let mut out = ev;
        out.flags = ev.flags.control_to_command();
        // If the event still lacked command (no control flag either), force it.
        if self.switcher.is_some() {
            out.flags.command = true;
            out.flags.control = false;
        }
        vec![Action::Suppress, Action::Post(out)]
    }
}

/// macOS virtual keycode for the ANSI 3 key (kVK_ANSI_3).
const DIGIT_3: Key = Key::Other(0x14);

/// Control is the only chord modifier (caps lock is ignored).
fn is_control_only(f: Flags) -> bool {
    f.control && !f.command && !f.shift && !f.option
}

fn is_arrow(key: Key) -> bool {
    matches!(key, Key::Left | Key::Right | Key::Up | Key::Down)
}

fn is_switcher_nav(key: Key) -> bool {
    is_arrow(key)
}

/// Apply a sequence of input events; return the flat action transcript.
pub fn run_sequence(cfg: &Config, inputs: &[Event]) -> Vec<Action> {
    let mut eng = Engine::new(cfg.clone());
    let mut out = Vec::new();
    for ev in inputs {
        out.extend(eng.handle(*ev));
    }
    out
}
