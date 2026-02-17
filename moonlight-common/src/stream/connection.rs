use std::{
    os::raw::{c_int, c_uchar, c_ushort},
    sync::Mutex,
};

use moonlight_common_sys::limelight::_CONNECTION_LISTENER_CALLBACKS;
use num::FromPrimitive;

use crate::stream::bindings::{ConnectionStatus, Stage};

pub trait ConnectionListener {
    /// This callback is invoked to indicate that a stage of initialization is about to begin
    fn stage_starting(&mut self, stage: Stage);
    /// This callback is invoked to indicate that a stage of initialization has completed
    fn stage_complete(&mut self, stage: Stage);

    /// This callback is invoked to indicate that a stage of initialization has failed.
    /// ConnListenerConnectionTerminated() will not be invoked because the connection was
    /// not yet fully established. LiInterruptConnection() and LiStopConnection() may
    /// result in this callback being invoked, but it is not guaranteed.
    fn stage_failed(&mut self, stage: Stage, error_code: i32);

    /// This callback is invoked after the connection is successfully established
    fn connection_started(&mut self);

    /// This callback is invoked when a connection is terminated after establishment.
    /// The errorCode will be 0 if the termination was reported to be intentional
    /// from the server (for example, the user closed the game). If errorCode is
    /// non-zero, it means the termination was probably unexpected (loss of network,
    /// crash, or similar conditions). This will not be invoked as a result of a call
    /// to LiStopConnection() or LiInterruptConnection().
    /// HINT: Use TerminationError
    fn connection_terminated(&mut self, error_code: i32);

    /// This callback is invoked to log debug message
    fn log_message(&mut self, message: &str);

    /// This callback is used to notify the client of a connection status change.
    /// Consider displaying an overlay for the user to notify them why their stream
    /// is not performing as expected.
    fn connection_status_update(&mut self, status: ConnectionStatus);

    /// This callback is invoked to notify the client of a change in HDR mode on
    /// the host. The client will probably want to update the local display mode
    /// to match the state of HDR on the host. This callback may be invoked even
    /// if the stream is not using an HDR-capable codec.
    fn set_hdr_mode(&mut self, hdr_enabled: bool);

    /// This callback is invoked to rumble a gamepad. The rumble effect values
    /// set in this callback are expected to persist until a future call sets a
    /// different haptic effect or turns off the motors by passing 0 for both
    /// motors. It is possible to receive rumble events for gamepads that aren't
    /// physically present, so your callback should handle this possibility.
    fn controller_rumble(
        &mut self,
        controller_number: u16,
        low_frequency_motor: u16,
        high_frequency_motor: u16,
    );

    /// This callback is invoked to rumble a gamepad's triggers. For more details,
    /// see the comment above on ConnListenerRumble().
    fn controller_rumble_triggers(
        &mut self,
        controller_number: u16,
        left_trigger_motor: u16,
        right_trigger_motor: u16,
    );

    /// This callback is invoked to notify the client that the host would like motion
    /// sensor reports for the specified gamepad (see LiSendControllerMotionEvent())
    /// at the specified reporting rate (or as close as possible).
    ///
    /// If reportRateHz is 0, the host is asking for motion event reporting to stop.
    fn controller_set_motion_event_state(
        &mut self,
        controller_number: u16,
        motion_type: u8,
        report_rate_hz: u16,
    );

    /// This callback is invoked to notify the client of a change in the dualsense
    /// adaptive trigger configuration.
    fn controller_set_adaptive_triggers(
        &mut self,
        controller_number: u16,
        event_flags: u8,
        type_left: u8,
        type_right: u8,
        left: &mut u8,
        right: &mut u8,
    );

    /// This callback is invoked to set a controller's RGB LED (if present).
    fn controller_set_led(&mut self, controller_number: u16, r: u8, g: u8, b: u8);
}

static GLOBAL_CONNECTION_LISTENER: Mutex<Option<Box<dyn ConnectionListener + Send + 'static>>> =
    Mutex::new(None);

fn global_listener<R>(f: impl FnOnce(&mut dyn ConnectionListener) -> R) -> R {
    let lock = GLOBAL_CONNECTION_LISTENER.lock();
    let mut lock = lock.expect("global connection listener");

    let listener = lock.as_mut().expect("global connection listener");
    f(listener.as_mut())
}

pub(crate) fn set_global(listener: impl ConnectionListener + Send + 'static) {
    let mut global_listener = GLOBAL_CONNECTION_LISTENER
        .lock()
        .expect("global connection lock");

    *global_listener = Some(Box::new(listener));
}
pub(crate) fn clear_global() {
    let mut decoder = GLOBAL_CONNECTION_LISTENER
        .lock()
        .expect("global video decoder");

    *decoder = None;
}

unsafe extern "C" fn stage_starting(stage: c_int) {
    global_listener(|listener| {
        listener.stage_starting(Stage::from_i32(stage).expect("valid stage"));
    });
}
unsafe extern "C" fn stage_complete(stage: c_int) {
    global_listener(|listener| {
        listener.stage_complete(Stage::from_i32(stage).expect("valid stage"));
    });
}
unsafe extern "C" fn stage_failed(stage: c_int, error_code: c_int) {
    global_listener(|listener| {
        listener.stage_failed(Stage::from_i32(stage).expect("valid stage"), error_code);
    });
}
unsafe extern "C" fn connection_started() {
    global_listener(|listener| {
        listener.connection_started();
    });
}
unsafe extern "C" fn connection_terminated(error_code: c_int) {
    global_listener(|listener| {
        listener.connection_terminated(error_code);
    });
}
unsafe extern "C" fn connection_status_update(status: c_int) {
    global_listener(|listener| {
        listener.connection_status_update(
            ConnectionStatus::from_i32(status).expect("valid connection status"),
        );
    });
}

// NOTE: The log_message callback requires C variadic arguments (c_variadic feature),
// which is still unstable in Rust. Setting logMessage to None in raw_callbacks() until
// c_variadic is stabilized. The ConnectionListener::log_message trait method still exists
// for forward compatibility.

unsafe extern "C" fn set_hdr_mode(hdr_enabled: bool) {
    global_listener(|listener| {
        listener.set_hdr_mode(hdr_enabled);
    })
}

unsafe extern "C" fn controller_rumble(
    controller_number: c_ushort,
    low_frequency_motor: c_ushort,
    high_frequency_motor: c_ushort,
) {
    global_listener(|listener| {
        listener.controller_rumble(controller_number, low_frequency_motor, high_frequency_motor);
    });
}
unsafe extern "C" fn controller_rumble_triggers(
    controller_number: c_ushort,
    left_trigger_motor: c_ushort,
    right_trigger_motor: c_ushort,
) {
    global_listener(|listener| {
        listener.controller_rumble_triggers(
            controller_number,
            left_trigger_motor,
            right_trigger_motor,
        );
    });
}
unsafe extern "C" fn controller_set_motion_event_state(
    controller_number: c_ushort,
    motion_type: c_uchar,
    report_rate_hz: c_ushort,
) {
    global_listener(|listener| {
        listener.controller_set_motion_event_state(controller_number, motion_type, report_rate_hz);
    })
}
unsafe extern "C" fn controller_set_led(controller_number: u16, r: u8, g: u8, b: u8) {
    global_listener(|listener| {
        listener.controller_set_led(controller_number, r, g, b);
    })
}
unsafe extern "C" fn controller_set_adaptive_triggers(
    controller_number: c_ushort,
    event_flags: c_uchar,
    type_left: c_uchar,
    type_right: c_uchar,
    left: *mut c_uchar,
    right: *mut c_uchar,
) {
    global_listener(|listener| {
        let (left, right) = unsafe { (&mut *left, &mut *right) };

        listener.controller_set_adaptive_triggers(
            controller_number,
            event_flags,
            type_left,
            type_right,
            left,
            right,
        );
    })
}

pub(crate) unsafe fn raw_callbacks() -> _CONNECTION_LISTENER_CALLBACKS {
    _CONNECTION_LISTENER_CALLBACKS {
        stageStarting: Some(stage_starting),
        stageComplete: Some(stage_complete),
        stageFailed: Some(stage_failed),
        connectionStarted: Some(connection_started),
        connectionTerminated: Some(connection_terminated),
        logMessage: None, // requires c_variadic (unstable)
        rumble: Some(controller_rumble),
        connectionStatusUpdate: Some(connection_status_update),
        setHdrMode: Some(set_hdr_mode),
        rumbleTriggers: Some(controller_rumble_triggers),
        setMotionEventState: Some(controller_set_motion_event_state),
        setControllerLED: Some(controller_set_led),
        setAdaptiveTriggers: Some(controller_set_adaptive_triggers),
    }
}
