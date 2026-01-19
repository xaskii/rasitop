use std::ffi::c_void;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use objc::declare::ClassDecl;
use objc::rc::StrongPtr;
use objc::runtime::{
    BOOL, Class, NO, Object, Sel, YES, objc_autoreleasePoolPop, objc_autoreleasePoolPush,
};
use objc::{Encode, Encoding, class, msg_send, sel, sel_impl};

use rasitop::metrics::{CoreSample, Sample, Sampler};

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {}

type Id = *mut Object;
const NIL: Id = std::ptr::null_mut();

const SAMPLE_INTERVAL_MS: u64 = 1000;
const POPOVER_WIDTH: f64 = 320.0;
const POPOVER_HEIGHT: f64 = 280.0;
const PADDING: f64 = 14.0;
const LABEL_WIDTH: f64 = 78.0;
const CORE_ROW_SPACING: f64 = 26.0;
const CORE_BARS_HEIGHT: f64 = 26.0;
const CORE_BARS_OFFSET: f64 = -5.0;
const FONT_NAME: &str = "JetBrains Mono";
const STATUS_GRAPH_HEIGHT: f64 = 14.0;
const STATUS_GRAPH_MIN_WIDTH: f64 = 24.0;
const STATUS_GRAPH_PADDING: f64 = 1.0;
const STATUS_GRAPH_GAP: f64 = 2.0;
const STATUS_GRAPH_BAR_WIDTH: f64 = 2.0;
const NS_APPLICATION_ACTIVATION_POLICY_ACCESSORY: i64 = 1;
const NS_VARIABLE_STATUS_ITEM_LENGTH: f64 = -1.0;
const NS_VISUAL_EFFECT_MATERIAL_HUD: i64 = 6;
const NS_VISUAL_EFFECT_BLENDING_MODE_BEHIND_WINDOW: i64 = 1;
const NS_VISUAL_EFFECT_STATE_ACTIVE: i64 = 1;
const NS_BOX_SEPARATOR: i64 = 2;
const NS_POPOVER_BEHAVIOR_TRANSIENT: i64 = 1;
const NS_IMAGE_LEFT: i64 = 2;
const NS_TEXT_ALIGNMENT_RIGHT: i64 = 1;
const NS_UTF8_STRING_ENCODING: usize = 4;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct NSPoint {
    x: f64,
    y: f64,
}

impl NSPoint {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

unsafe impl Encode for NSPoint {
    fn encode() -> Encoding {
        let encoding = format!(
            "{{CGPoint={}{}}}",
            f64::encode().as_str(),
            f64::encode().as_str()
        );
        unsafe { Encoding::from_str(&encoding) }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct NSSize {
    width: f64,
    height: f64,
}

impl NSSize {
    fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

unsafe impl Encode for NSSize {
    fn encode() -> Encoding {
        let encoding = format!(
            "{{CGSize={}{}}}",
            f64::encode().as_str(),
            f64::encode().as_str()
        );
        unsafe { Encoding::from_str(&encoding) }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

impl NSRect {
    fn new(origin: NSPoint, size: NSSize) -> Self {
        Self { origin, size }
    }
}

unsafe impl Encode for NSRect {
    fn encode() -> Encoding {
        let encoding = format!(
            "{{CGRect={}{}}}",
            NSPoint::encode().as_str(),
            NSSize::encode().as_str()
        );
        unsafe { Encoding::from_str(&encoding) }
    }
}

struct AutoreleasePool(*mut c_void);

impl AutoreleasePool {
    fn new() -> Self {
        let ctx = unsafe { objc_autoreleasePoolPush() };
        Self(ctx)
    }
}

impl Drop for AutoreleasePool {
    fn drop(&mut self) {
        unsafe {
            objc_autoreleasePoolPop(self.0);
        }
    }
}

#[derive(Default)]
struct SampleState {
    sample: Option<Sample>,
    last_update: Option<Instant>,
    last_error: Option<String>,
}

struct UiHandles {
    status_button: Id,
    status_line: Id,
    total_power: Id,
    power_line: Id,
    busy_line: Id,
    freq_line: Id,
    temp_line: Id,
    mem_line: Id,
    core_summary: Id,
    core_bars_view: Id,
    core_bars_state: Arc<Mutex<CoreBarsState>>,
    error_line: Id,
}

#[derive(Clone, Copy, Debug)]
enum CoreCluster {
    Efficiency,
    Performance,
    Unknown,
}

#[derive(Clone, Debug)]
struct CoreBar {
    busy_ratio: f64,
    cluster: CoreCluster,
}

#[derive(Default)]
struct CoreBarsState {
    bars: Vec<CoreBar>,
}

struct TimerState {
    shared: Arc<Mutex<SampleState>>,
    ui: UiHandles,
}

fn main() {
    if std::env::consts::OS != "macos" {
        eprintln!("rasitop-menubar requires macOS.");
        return;
    }

    unsafe {
        let _pool = AutoreleasePool::new();
        let app: Id = msg_send![class!(NSApplication), sharedApplication];
        let _: BOOL = msg_send![
            app,
            setActivationPolicy: NS_APPLICATION_ACTIVATION_POLICY_ACCESSORY
        ];

        let shared = Arc::new(Mutex::new(SampleState::default()));
        start_sampler_thread(shared.clone());

        let (popover, mut ui_handles) = build_popover();

        let status_bar: Id = msg_send![class!(NSStatusBar), systemStatusBar];
        let status_item: Id = msg_send![
            status_bar,
            statusItemWithLength: NS_VARIABLE_STATUS_ITEM_LENGTH
        ];
        let status_button: Id = msg_send![status_item, button];
        if status_button != NIL {
            set_button_title(status_button, "--.-W");
            let status_font = font_named(FONT_NAME, 11.0);
            set_button_font(status_button, status_font);
            let _: () = msg_send![status_button, setImagePosition: NS_IMAGE_LEFT];
        }

        ui_handles.status_button = status_button;

        let status_target = make_status_target(popover, status_item);
        if status_button != NIL {
            let _: () = msg_send![status_button, setTarget: *status_target];
            let _: () = msg_send![status_button, setAction: sel!(togglePopover:)];
        }

        let timer_target = make_timer_target(shared, ui_handles);
        let _: Id = msg_send![
            class!(NSTimer),
            scheduledTimerWithTimeInterval: 1.0
            target: *timer_target
            selector: sel!(tick:)
            userInfo: NIL
            repeats: YES
        ];

        let _: () = msg_send![app, run];
    }
}

fn start_sampler_thread(shared: Arc<Mutex<SampleState>>) {
    thread::spawn(move || {
        if std::env::consts::OS != "macos" {
            let mut state = shared.lock().unwrap();
            state.last_error = Some("Unsupported OS.".to_string());
            return;
        }

        let mut sampler = match Sampler::new() {
            Ok(sampler) => sampler,
            Err(err) => {
                let mut state = shared.lock().unwrap();
                state.last_error = Some(format!("Sampler init failed: {err}"));
                return;
            }
        };

        loop {
            match sampler.sample(SAMPLE_INTERVAL_MS as u32) {
                Ok(sample) => {
                    let mut state = shared.lock().unwrap();
                    state.sample = Some(sample);
                    state.last_update = Some(Instant::now());
                    state.last_error = None;
                }
                Err(err) => {
                    let mut state = shared.lock().unwrap();
                    state.last_error = Some(format!("Sample error: {err}"));
                    drop(state);
                    thread::sleep(Duration::from_millis(SAMPLE_INTERVAL_MS));
                }
            }
        }
    });
}

fn build_popover() -> (Id, UiHandles) {
    unsafe {
        let content: Id = msg_send![class!(NSView), alloc];
        let content: Id = msg_send![content, initWithFrame: NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(POPOVER_WIDTH, POPOVER_HEIGHT),
        )];

        let background: Id = msg_send![class!(NSVisualEffectView), alloc];
        let background: Id = msg_send![
            background,
            initWithFrame: NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(POPOVER_WIDTH, POPOVER_HEIGHT)
            )
        ];
        let _: () = msg_send![background, setMaterial: NS_VISUAL_EFFECT_MATERIAL_HUD];
        let _: () =
            msg_send![background, setBlendingMode: NS_VISUAL_EFFECT_BLENDING_MODE_BEHIND_WINDOW];
        let _: () = msg_send![background, setState: NS_VISUAL_EFFECT_STATE_ACTIVE];
        let _: () = msg_send![content, addSubview: background];

        let label_color: Id = msg_send![class!(NSColor), labelColor];
        let secondary_color: Id = msg_send![class!(NSColor), secondaryLabelColor];
        let accent_color: Id = msg_send![class!(NSColor), systemBlueColor];
        let error_color: Id = msg_send![class!(NSColor), systemRedColor];

        let subtitle_font = font_named(FONT_NAME, 11.0);
        let total_font = font_named(FONT_NAME, 18.0);
        let label_font = font_named(FONT_NAME, 10.0);
        let value_font = font_named(FONT_NAME, 11.0);
        let core_bars_state = Arc::new(Mutex::new(CoreBarsState::default()));

        let total_y = POPOVER_HEIGHT - PADDING - 24.0;
        let status_y = total_y - 18.0;
        let divider_y = status_y - 16.0;
        let mut row_y = divider_y - 18.0;

        let total_power = make_label(
            "--.-W",
            NSRect::new(
                NSPoint::new(PADDING, total_y),
                NSSize::new(POPOVER_WIDTH - 2.0 * PADDING, 20.0),
            ),
            total_font,
            accent_color,
            false,
        );
        let _: () = msg_send![background, addSubview: total_power];

        let status_line = make_label(
            "Waiting for first sample...",
            NSRect::new(
                NSPoint::new(PADDING, status_y),
                NSSize::new(POPOVER_WIDTH - 2.0 * PADDING, 14.0),
            ),
            subtitle_font,
            secondary_color,
            false,
        );
        let _: () = msg_send![background, addSubview: status_line];

        let divider: Id = msg_send![class!(NSBox), alloc];
        let divider: Id = msg_send![
            divider,
            initWithFrame: NSRect::new(
                NSPoint::new(PADDING, divider_y),
                NSSize::new(POPOVER_WIDTH - 2.0 * PADDING, 1.0)
            )
        ];
        let _: () = msg_send![divider, setBoxType: NS_BOX_SEPARATOR];
        let _: () = msg_send![background, addSubview: divider];

        let power_line = add_row(
            background,
            "POWER",
            row_y,
            label_font,
            value_font,
            secondary_color,
            label_color,
        );
        row_y -= 18.0;

        let busy_line = add_row(
            background,
            "CPU BUSY",
            row_y,
            label_font,
            value_font,
            secondary_color,
            label_color,
        );
        row_y -= 18.0;

        let freq_line = add_row(
            background,
            "FREQ",
            row_y,
            label_font,
            value_font,
            secondary_color,
            label_color,
        );
        row_y -= 18.0;

        let temp_line = add_row(
            background,
            "TEMP",
            row_y,
            label_font,
            value_font,
            secondary_color,
            label_color,
        );
        row_y -= 18.0;

        let mem_line = add_row(
            background,
            "MEM",
            row_y,
            label_font,
            value_font,
            secondary_color,
            label_color,
        );

        row_y -= CORE_ROW_SPACING;

        let core_label = make_label(
            "CORES",
            NSRect::new(NSPoint::new(PADDING, row_y), NSSize::new(LABEL_WIDTH, 16.0)),
            label_font,
            secondary_color,
            false,
        );
        let _: () = msg_send![background, addSubview: core_label];

        let core_summary = make_label(
            "--",
            NSRect::new(
                NSPoint::new(PADDING + LABEL_WIDTH, row_y),
                NSSize::new(POPOVER_WIDTH - 2.0 * PADDING - LABEL_WIDTH, 16.0),
            ),
            value_font,
            label_color,
            false,
        );
        let _: () = msg_send![background, addSubview: core_summary];

        let core_bars_view = build_core_bars_view(
            core_bars_state.clone(),
            NSRect::new(
                NSPoint::new(PADDING, row_y + CORE_BARS_OFFSET),
                NSSize::new(POPOVER_WIDTH - 2.0 * PADDING, CORE_BARS_HEIGHT),
            ),
        );
        let _: () = msg_send![background, addSubview: core_bars_view];

        let error_line = make_label(
            "",
            NSRect::new(
                NSPoint::new(PADDING, PADDING + 6.0),
                NSSize::new(POPOVER_WIDTH - 2.0 * PADDING, 14.0),
            ),
            subtitle_font,
            error_color,
            false,
        );
        let _: () = msg_send![error_line, setHidden: YES];
        let _: () = msg_send![background, addSubview: error_line];

        let quit_button: Id = msg_send![class!(NSButton), alloc];
        let quit_button: Id = msg_send![quit_button, initWithFrame: NSRect::new(
            NSPoint::new(POPOVER_WIDTH - PADDING - 92.0, PADDING - 2.0),
            NSSize::new(92.0, 22.0),
        )];
        let _: () = msg_send![quit_button, setTitle: nsstring("Quit")];
        let app: Id = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![quit_button, setTarget: app];
        let _: () = msg_send![quit_button, setAction: sel!(terminate:)];
        let _: () = msg_send![quit_button, setBezelStyle: 1i64];
        let _: () = msg_send![quit_button, setFont: value_font];
        let _: () = msg_send![background, addSubview: quit_button];

        let view_controller: Id = msg_send![class!(NSViewController), new];
        let _: () = msg_send![view_controller, setView: content];

        let popover: Id = msg_send![class!(NSPopover), alloc];
        let popover: Id = msg_send![popover, init];
        let _: () = msg_send![popover, setContentViewController: view_controller];
        let _: () = msg_send![popover, setContentSize: NSSize::new(POPOVER_WIDTH, POPOVER_HEIGHT)];
        let _: () = msg_send![popover, setBehavior: NS_POPOVER_BEHAVIOR_TRANSIENT];
        let _: () = msg_send![popover, setAnimates: YES];

        (
            popover,
            UiHandles {
                status_button: NIL,
                status_line,
                total_power,
                power_line,
                busy_line,
                freq_line,
                temp_line,
                mem_line,
                core_summary,
                core_bars_view,
                core_bars_state,
                error_line,
            },
        )
    }
}

fn make_label(text: &str, frame: NSRect, font: Id, color: Id, align_right: bool) -> Id {
    unsafe {
        let label: Id = msg_send![class!(NSTextField), alloc];
        let label: Id = msg_send![label, initWithFrame: frame];
        let _: () = msg_send![label, setStringValue: nsstring(text)];
        let _: () = msg_send![label, setBezeled: NO];
        let _: () = msg_send![label, setDrawsBackground: NO];
        let _: () = msg_send![label, setEditable: NO];
        let _: () = msg_send![label, setSelectable: NO];
        let _: () = msg_send![label, setFont: font];
        let _: () = msg_send![label, setTextColor: color];
        if align_right {
            let _: () = msg_send![label, setAlignment: NS_TEXT_ALIGNMENT_RIGHT];
        }
        label
    }
}

fn add_row(
    parent: Id,
    label: &str,
    y: f64,
    label_font: Id,
    value_font: Id,
    label_color: Id,
    value_color: Id,
) -> Id {
    unsafe {
        let label_frame = NSRect::new(NSPoint::new(PADDING, y), NSSize::new(LABEL_WIDTH, 16.0));
        let value_frame = NSRect::new(
            NSPoint::new(PADDING + LABEL_WIDTH, y),
            NSSize::new(POPOVER_WIDTH - 2.0 * PADDING - LABEL_WIDTH, 16.0),
        );
        let label_view = make_label(label, label_frame, label_font, label_color, false);
        let value_view = make_label("--", value_frame, value_font, value_color, false);
        let _: () = msg_send![parent, addSubview: label_view];
        let _: () = msg_send![parent, addSubview: value_view];
        value_view
    }
}

fn build_core_bars_view(state: Arc<Mutex<CoreBarsState>>, frame: NSRect) -> Id {
    unsafe {
        let class = core_bars_view_class();
        let view: Id = msg_send![class, alloc];
        let view: Id = msg_send![view, initWithFrame: frame];
        let state_ptr = Box::into_raw(Box::new(state)) as *mut c_void;
        (*view).set_ivar("state", state_ptr);
        view
    }
}

fn core_bars_view_class() -> &'static Class {
    use std::sync::Once;
    static mut CLASS: *const Class = std::ptr::null::<Class>();
    static ONCE: Once = Once::new();

    ONCE.call_once(|| unsafe {
        let mut decl = ClassDecl::new("RasitopCoreBarsView", class!(NSView)).unwrap();
        decl.add_ivar::<*mut c_void>("state");
        decl.add_method(
            sel!(drawRect:),
            draw_core_bars as extern "C" fn(&Object, Sel, NSRect),
        );
        CLASS = decl.register();
    });

    unsafe { &*CLASS }
}

extern "C" fn draw_core_bars(this: &Object, _: Sel, _: NSRect) {
    unsafe {
        let state_ptr: *mut c_void = *this.get_ivar("state");
        if state_ptr.is_null() {
            return;
        }
        let state = &*(state_ptr as *mut Arc<Mutex<CoreBarsState>>);
        let bars = {
            let state = state.lock().unwrap();
            state.bars.clone()
        };
        let view: Id = this as *const _ as Id;
        let bounds: NSRect = msg_send![view, bounds];
        draw_core_bars_in_bounds(&bars, bounds);
    }
}

fn draw_core_bars_in_bounds(bars: &[CoreBar], bounds: NSRect) {
    draw_core_bars_with_style(bars, bounds, 2.0, 3.0, 0.25);
}

fn draw_core_bars_with_style(
    bars: &[CoreBar],
    bounds: NSRect,
    padding: f64,
    gap: f64,
    base_alpha: f64,
) {
    if bars.is_empty() {
        return;
    }

    let count = bars.len() as f64;
    let max_width = bounds.size.width - (padding * 2.0) - gap * (count - 1.0);
    let bar_width = if max_width > 0.0 {
        (max_width / count).max(1.0)
    } else {
        1.0
    };
    let max_height = (bounds.size.height - padding * 2.0).max(1.0);

    let base_color: Id = unsafe { msg_send![class!(NSColor), tertiaryLabelColor] };
    let base_color = color_with_alpha(base_color, base_alpha);

    for (index, bar) in bars.iter().enumerate() {
        let x = bounds.origin.x + padding + (bar_width + gap) * index as f64;
        let base_rect = NSRect::new(
            NSPoint::new(x, bounds.origin.y + padding),
            NSSize::new(bar_width, max_height),
        );
        let base_radius = bar_width.min(4.0).min(max_height / 2.0);
        fill_rounded_rect(base_rect, base_radius, base_color);

        let ratio = bar.busy_ratio.clamp(0.0, 1.0);
        let height = (max_height * ratio).max(1.0);
        let fill_rect = NSRect::new(
            NSPoint::new(x, bounds.origin.y + padding),
            NSSize::new(bar_width, height),
        );
        let fill_radius = bar_width.min(4.0).min(height / 2.0);
        let fill_color = color_with_alpha(core_bar_color(bar.cluster), 0.9);
        fill_rounded_rect(fill_rect, fill_radius, fill_color);
    }
}

fn fill_rounded_rect(rect: NSRect, radius: f64, color: Id) {
    unsafe {
        let path: Id = msg_send![
            class!(NSBezierPath),
            bezierPathWithRoundedRect: rect
            xRadius: radius
            yRadius: radius
        ];
        let _: () = msg_send![color, setFill];
        let _: () = msg_send![path, fill];
    }
}

fn color_with_alpha(color: Id, alpha: f64) -> Id {
    unsafe { msg_send![color, colorWithAlphaComponent: alpha] }
}

fn core_bar_color(cluster: CoreCluster) -> Id {
    unsafe {
        match cluster {
            CoreCluster::Efficiency => msg_send![class!(NSColor), systemTealColor],
            CoreCluster::Performance => msg_send![class!(NSColor), systemBlueColor],
            CoreCluster::Unknown => msg_send![class!(NSColor), systemGrayColor],
        }
    }
}

fn build_status_graph_image(bars: &[CoreBar]) -> Option<Id> {
    if bars.is_empty() {
        return None;
    }

    let count = bars.len() as f64;
    let width = (STATUS_GRAPH_PADDING * 2.0
        + STATUS_GRAPH_BAR_WIDTH * count
        + STATUS_GRAPH_GAP * (count - 1.0))
        .max(STATUS_GRAPH_MIN_WIDTH);
    let size = NSSize::new(width, STATUS_GRAPH_HEIGHT);

    unsafe {
        let image: Id = msg_send![class!(NSImage), alloc];
        let image: Id = msg_send![image, initWithSize: size];
        let _: () = msg_send![image, setTemplate: NO];
        let _: () = msg_send![image, lockFocus];
        draw_core_bars_with_style(
            bars,
            NSRect::new(NSPoint::new(0.0, 0.0), size),
            STATUS_GRAPH_PADDING,
            STATUS_GRAPH_GAP,
            0.2,
        );
        let _: () = msg_send![image, unlockFocus];
        let image: Id = msg_send![image, autorelease];
        Some(image)
    }
}

fn make_status_target(popover: Id, status_item: Id) -> StrongPtr {
    unsafe {
        let class = status_target_class();
        let target: Id = msg_send![class, new];
        (*target).set_ivar("popover", popover);
        (*target).set_ivar("status_item", status_item);
        StrongPtr::new(target)
    }
}

fn status_target_class() -> &'static Class {
    use std::sync::Once;
    static mut CLASS: *const Class = std::ptr::null::<Class>();
    static ONCE: Once = Once::new();

    ONCE.call_once(|| unsafe {
        let mut decl = ClassDecl::new("RasitopStatusTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<Id>("popover");
        decl.add_ivar::<Id>("status_item");
        decl.add_method(
            sel!(togglePopover:),
            toggle_popover as extern "C" fn(&Object, Sel, Id),
        );
        CLASS = decl.register();
    });

    unsafe { &*CLASS }
}

extern "C" fn toggle_popover(this: &Object, _: Sel, _: Id) {
    unsafe {
        let popover: Id = *this.get_ivar("popover");
        let status_item: Id = *this.get_ivar("status_item");
        if popover == NIL || status_item == NIL {
            return;
        }
        let button: Id = msg_send![status_item, button];
        if button == NIL {
            return;
        }
        let shown: BOOL = msg_send![popover, isShown];
        if shown == YES {
            let _: () = msg_send![popover, close];
        } else {
            let bounds: NSRect = msg_send![button, bounds];
            let _: () = msg_send![
                popover,
                showRelativeToRect: bounds
                ofView: button
                preferredEdge: 3i64
            ];
        }
    }
}

fn make_timer_target(shared: Arc<Mutex<SampleState>>, ui: UiHandles) -> StrongPtr {
    unsafe {
        let class = timer_target_class();
        let target: Id = msg_send![class, new];
        let state = Box::new(TimerState { shared, ui });
        let state_ptr = Box::into_raw(state) as *mut c_void;
        (*target).set_ivar("state", state_ptr);
        StrongPtr::new(target)
    }
}

fn timer_target_class() -> &'static Class {
    use std::sync::Once;
    static mut CLASS: *const Class = std::ptr::null::<Class>();
    static ONCE: Once = Once::new();

    ONCE.call_once(|| unsafe {
        let mut decl = ClassDecl::new("RasitopTimerTarget", class!(NSObject)).unwrap();
        decl.add_ivar::<*mut c_void>("state");
        decl.add_method(sel!(tick:), tick as extern "C" fn(&Object, Sel, Id));
        CLASS = decl.register();
    });

    unsafe { &*CLASS }
}

extern "C" fn tick(this: &Object, _: Sel, _: Id) {
    unsafe {
        let _pool = AutoreleasePool::new();
        let state_ptr: *mut c_void = *this.get_ivar("state");
        if state_ptr.is_null() {
            return;
        }
        let state = &mut *(state_ptr as *mut TimerState);
        state.update();
    }
}

impl TimerState {
    fn update(&mut self) {
        let (sample, updated_at, last_error) = {
            let state = self.shared.lock().unwrap();
            (
                state.sample.clone(),
                state.last_update,
                state.last_error.clone(),
            )
        };

        let elapsed_secs = updated_at.map(|when| when.elapsed().as_secs_f64());
        let ui_strings = format_ui_strings(sample.as_ref(), elapsed_secs, last_error.as_deref());

        set_label_text(self.ui.status_line, &ui_strings.status_line);
        set_label_text(self.ui.total_power, &ui_strings.total_power);
        set_label_text(self.ui.power_line, &ui_strings.power_line);
        set_label_text(self.ui.busy_line, &ui_strings.busy_line);
        set_label_text(self.ui.freq_line, &ui_strings.freq_line);
        set_label_text(self.ui.temp_line, &ui_strings.temp_line);
        set_label_text(self.ui.mem_line, &ui_strings.mem_line);
        set_label_text(self.ui.core_summary, &ui_strings.core_summary);

        let core_bars =
            core_bars_from_samples(sample.as_ref().and_then(|item| item.cpu_cores.as_deref()));
        update_core_bars(&self.ui.core_bars_state, self.ui.core_bars_view, &core_bars);

        if self.ui.status_button != NIL {
            set_button_title(self.ui.status_button, &ui_strings.status_title);
            update_status_graph(self.ui.status_button, &core_bars);
        }

        if let Some(err) = ui_strings.error_line.as_ref() {
            set_label_text(self.ui.error_line, err);
            set_label_hidden(self.ui.error_line, false);
        } else {
            set_label_text(self.ui.error_line, "");
            set_label_hidden(self.ui.error_line, true);
        }
    }
}

fn nsstring(text: &str) -> Id {
    unsafe {
        let bytes = text.as_bytes();
        let ns: Id = msg_send![class!(NSString), alloc];
        let ns: Id = msg_send![
            ns,
            initWithBytes: bytes.as_ptr()
            length: bytes.len()
            encoding: NS_UTF8_STRING_ENCODING
        ];
        let ns: Id = msg_send![ns, autorelease];
        ns
    }
}

fn font_named(name: &str, size: f64) -> Id {
    unsafe {
        let ns_name = nsstring(name);
        let font: Id = msg_send![class!(NSFont), fontWithName: ns_name size: size];
        if font != NIL {
            font
        } else {
            msg_send![class!(NSFont), systemFontOfSize: size]
        }
    }
}

fn set_label_text(label: Id, text: &str) {
    unsafe {
        let _: () = msg_send![label, setStringValue: nsstring(text)];
    }
}

fn set_label_hidden(label: Id, hidden: bool) {
    unsafe {
        let flag = if hidden { YES } else { NO };
        let _: () = msg_send![label, setHidden: flag];
    }
}

fn set_button_title(button: Id, text: &str) {
    unsafe {
        let _: () = msg_send![button, setTitle: nsstring(text)];
    }
}

fn set_button_font(button: Id, font: Id) {
    unsafe {
        let _: () = msg_send![button, setFont: font];
    }
}

fn format_power(mw: f64) -> String {
    format!("{:.1}W", mw / 1000.0)
}

fn format_ratio(value: Option<f64>) -> String {
    value
        .map(|v| format!("{:.0}%", v * 100.0))
        .unwrap_or_else(|| "--".to_string())
}

fn format_freq(value: Option<f64>) -> String {
    value
        .map(|v| format!("{:.2}GHz", v / 1e9))
        .unwrap_or_else(|| "--".to_string())
}

fn format_temp(value: Option<f32>) -> String {
    value
        .map(|v| format!("{:.1}C", v))
        .unwrap_or_else(|| "--".to_string())
}

fn format_mem(usage: Option<u64>, total: Option<u64>) -> String {
    match (usage, total) {
        (Some(usage), Some(total)) if total > 0 => {
            let usage_gb = usage as f64 / (1024.0 * 1024.0 * 1024.0);
            let total_gb = total as f64 / (1024.0 * 1024.0 * 1024.0);
            format!("{:.1}/{:.1}GB", usage_gb, total_gb)
        }
        _ => "--".to_string(),
    }
}

#[derive(Debug, PartialEq)]
struct UiStrings {
    status_line: String,
    total_power: String,
    power_line: String,
    busy_line: String,
    freq_line: String,
    temp_line: String,
    mem_line: String,
    core_summary: String,
    error_line: Option<String>,
    status_title: String,
}

fn core_bars_from_samples(cores: Option<&[CoreSample]>) -> Vec<CoreBar> {
    let mut bars = Vec::new();
    if let Some(cores) = cores {
        bars.reserve(cores.len());
        for core in cores {
            bars.push(CoreBar {
                busy_ratio: core.busy_ratio,
                cluster: core_cluster_from_label(&core.label),
            });
        }
    }
    bars
}

fn update_core_bars(state: &Arc<Mutex<CoreBarsState>>, view: Id, bars: &[CoreBar]) {
    let mut guard = state.lock().unwrap();
    guard.bars = bars.to_vec();
    drop(guard);

    if view != NIL {
        unsafe {
            let _: () = msg_send![view, setNeedsDisplay: YES];
        }
    }
}

fn update_status_graph(button: Id, bars: &[CoreBar]) {
    if button == NIL {
        return;
    }
    let image = build_status_graph_image(bars);
    unsafe {
        let _: () = msg_send![button, setImage: image.unwrap_or(NIL)];
    }
}

fn core_cluster_from_label(label: &str) -> CoreCluster {
    if label.starts_with('E') {
        CoreCluster::Efficiency
    } else if label.starts_with('P') {
        CoreCluster::Performance
    } else {
        CoreCluster::Unknown
    }
}

fn format_core_summary(cores: Option<&[CoreSample]>) -> String {
    match cores {
        None | Some([]) => "--".to_string(),
        Some(cores) => {
            let mut e_count = 0;
            let mut p_count = 0;
            for core in cores {
                match core_cluster_from_label(&core.label) {
                    CoreCluster::Efficiency => e_count += 1,
                    CoreCluster::Performance => p_count += 1,
                    CoreCluster::Unknown => {}
                }
            }
            if e_count > 0 || p_count > 0 {
                format!("E{} P{}", e_count, p_count)
            } else {
                format!("{} cores", cores.len())
            }
        }
    }
}

fn format_ui_strings(
    sample: Option<&Sample>,
    elapsed_secs: Option<f64>,
    last_error: Option<&str>,
) -> UiStrings {
    let status_line = match elapsed_secs {
        Some(elapsed) => format!("Updated {:.1}s ago", elapsed),
        None => "Waiting for first sample...".to_string(),
    };

    let (
        total_power,
        power_line,
        busy_line,
        freq_line,
        temp_line,
        mem_line,
        core_summary,
        status_title,
    ) = if let Some(sample) = sample {
        let total_power = match sample.sys_power_mw {
            Some(sys) => format_power(sys),
            None => format_power(sample.combined_power_mw),
        };
        let mut power_line = format!(
            "CPU {}  GPU {}  ANE {}",
            format_power(sample.cpu_power_mw),
            format_power(sample.gpu_power_mw),
            format_power(sample.ane_power_mw),
        );
        power_line.push_str(&format!(
            "  Total {}",
            format_power(sample.combined_power_mw)
        ));

        let busy_line = format!(
            "E {}  P {}",
            format_ratio(sample.e_busy_ratio),
            format_ratio(sample.p_busy_ratio),
        );
        let freq_line = format!(
            "E {}  P {}",
            format_freq(sample.e_freq_hz),
            format_freq(sample.p_freq_hz),
        );
        let temp_line = format!(
            "CPU {}  GPU {}",
            format_temp(sample.cpu_temp_c),
            format_temp(sample.gpu_temp_c),
        );
        let mem_line = format!(
            "RAM {}  Swap {}",
            format_mem(sample.ram_usage_bytes, sample.ram_total_bytes),
            format_mem(sample.swap_usage_bytes, sample.swap_total_bytes),
        );
        let core_summary = format_core_summary(sample.cpu_cores.as_deref());
        let status_title = match sample.sys_power_mw {
            Some(sys) => format_power(sys),
            None => format_power(sample.combined_power_mw),
        };

        (
            total_power,
            power_line,
            busy_line,
            freq_line,
            temp_line,
            mem_line,
            core_summary,
            status_title,
        )
    } else {
        (
            "--.-W".to_string(),
            "CPU --  GPU --  ANE --  Total --".to_string(),
            "E --  P --".to_string(),
            "E --  P --".to_string(),
            "CPU --  GPU --".to_string(),
            "RAM --  Swap --".to_string(),
            "--".to_string(),
            "--.-W".to_string(),
        )
    };

    UiStrings {
        status_line,
        total_power,
        power_line,
        busy_line,
        freq_line,
        temp_line,
        mem_line,
        core_summary,
        error_line: last_error.map(|err| format!("Error: {}", err)),
        status_title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_debug_snapshot;

    fn sample_for_ui_test() -> Sample {
        Sample {
            timestamp: Some("2025-04-26T21:49:40Z".to_string()),
            cpu_power_mw: 1941.82,
            gpu_power_mw: 650.0,
            ane_power_mw: 120.0,
            combined_power_mw: 2711.82,
            sys_power_mw: Some(4123.5),
            cpu_energy: Some(500),
            gpu_energy: Some(200),
            ane_energy: Some(25),
            battery_percent: Some(69),
            cpu_busy_ratio: Some(0.5),
            e_busy_ratio: Some(0.388264),
            p_busy_ratio: Some(0.692109),
            e_freq_hz: Some(972_013_000.0),
            p_freq_hz: Some(3_089_180_000.0),
            cpu_temp_c: Some(48.25),
            gpu_temp_c: Some(42.75),
            ram_total_bytes: Some(25_769_803_776),
            ram_usage_bytes: Some(20_985_479_168),
            swap_total_bytes: Some(4_294_967_296),
            swap_usage_bytes: Some(2_602_434_560),
            cpu_cores: Some(vec![
                CoreSample {
                    label: "E0".to_string(),
                    busy_ratio: 0.12,
                },
                CoreSample {
                    label: "E1".to_string(),
                    busy_ratio: 0.48,
                },
                CoreSample {
                    label: "P0".to_string(),
                    busy_ratio: 0.71,
                },
                CoreSample {
                    label: "P1".to_string(),
                    busy_ratio: 0.64,
                },
            ]),
        }
    }

    #[test]
    fn format_mem_missing_total_returns_placeholder() {
        assert_eq!(format_mem(Some(1), None), "--");
    }

    #[test]
    fn snapshot_ui_strings_with_sample() {
        let sample = sample_for_ui_test();
        let ui = format_ui_strings(Some(&sample), Some(0.4), None);
        assert_debug_snapshot!(ui);
    }

    #[test]
    fn snapshot_ui_strings_with_error() {
        let ui = format_ui_strings(None, None, Some("boom"));
        assert_debug_snapshot!(ui);
    }
}
