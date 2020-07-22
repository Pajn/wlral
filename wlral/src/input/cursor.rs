use super::seat::SeatManager;
use crate::geometry::FPoint;
use crate::input::device::{Device, DeviceType};
use crate::input::event_filter::{EventFilter, EventFilterManager};
use crate::input::events::*;
use crate::{output_manager::OutputManager, window_manager::WindowManager};
use log::debug;
use std::cell::RefCell;
use std::ffi::CString;
use std::ops::Deref;
use std::pin::Pin;
use std::ptr;
use std::rc::Rc;
use wlroots_sys::*;

pub struct CursorManager {
  output_manager: Rc<OutputManager>,
  window_manager: Rc<WindowManager>,
  seat_manager: Rc<SeatManager>,
  event_filter_manager: Rc<RefCell<EventFilterManager>>,
  cursor: *mut wlr_cursor,
  cursor_mgr: *mut wlr_xcursor_manager,
  pointers: RefCell<Vec<Rc<Device>>>,

  event_manager: RefCell<Option<Pin<Box<CursorEventManager>>>>,
}

impl CursorManager {
  pub(crate) fn init(
    output_manager: Rc<OutputManager>,
    window_manager: Rc<WindowManager>,
    seat_manager: Rc<SeatManager>,
    event_filter_manager: Rc<RefCell<EventFilterManager>>,
    output_layout: *mut wlr_output_layout,
  ) -> Rc<CursorManager> {
    debug!("CursorManager::init");

    let cursor = unsafe { wlr_cursor_create() };
    unsafe {
      wlr_cursor_attach_output_layout(cursor, output_layout);
    };

    // Creates an xcursor manager, another wlroots utility which loads up
    // Xcursor themes to source cursor images from and makes sure that cursor
    // images are available at all scale factors on the screen (necessary for
    // HiDPI support). We add a cursor theme at scale factor 1 to begin with.
    let cursor_mgr = unsafe { wlr_xcursor_manager_create(ptr::null(), 24) };
    unsafe { wlr_xcursor_manager_load(cursor_mgr, 1.0) };

    let cursor_manager = Rc::new(CursorManager {
      output_manager: output_manager.clone(),
      window_manager,
      seat_manager: seat_manager.clone(),
      event_filter_manager,
      cursor,
      cursor_mgr,
      pointers: RefCell::new(vec![]),

      event_manager: RefCell::new(None),
    });

    output_manager
      .on_new_output()
      .subscribe(listener!(cursor_manager => move |output| {
        cursor_manager.refresh_device_mappings();
        output.on_destroy.then(listener!(cursor_manager => move || {
          cursor_manager.refresh_device_mappings();
        }));
      }));

    seat_manager
      .on_new_device
      .subscribe(listener!(cursor_manager => move |device| {
        if let DeviceType::Pointer(_) = device.device_type() {
          device.on_destroy.then(listener!(cursor_manager, device => move || {
            debug!("CursorManager::destroy_input_device");
            cursor_manager
              .pointers
              .borrow_mut()
              .retain(|pointer| pointer.deref() != device.deref());

            cursor_manager
              .seat_manager
              .set_has_any_pointer(cursor_manager.has_pointer_device());
          }));

          debug!("CursorManager::add_input_device");

          unsafe {
            wlr_cursor_attach_input_device(cursor, device.raw_ptr());
          }

          cursor_manager.pointers.borrow_mut().push(device.clone());

          cursor_manager.refresh_device_mappings();
          cursor_manager.seat_manager.set_has_any_pointer(true);
        }
      }));

    #[allow(unused_mut)]
    let mut event_manager = CursorEventManager::new(cursor_manager.clone());
    #[cfg(not(test))]
    unsafe {
      event_manager.request_set_cursor(&mut (*seat_manager.raw_seat()).events.request_set_cursor);
      event_manager.motion(&mut (*cursor).events.motion);
      event_manager.motion_absolute(&mut (*cursor).events.motion_absolute);
      event_manager.button(&mut (*cursor).events.button);
      event_manager.axis(&mut (*cursor).events.axis);
      event_manager.frame(&mut (*cursor).events.frame);
    }
    *cursor_manager.event_manager.borrow_mut() = Some(event_manager);

    cursor_manager
  }

  #[cfg(test)]
  pub(crate) fn mock(
    output_manager: Rc<OutputManager>,
    window_manager: Rc<WindowManager>,
    seat_manager: Rc<SeatManager>,
    event_filter_manager: Rc<RefCell<EventFilterManager>>,
    cursor: *mut wlr_cursor,
    cursor_mgr: *mut wlr_xcursor_manager,
  ) -> Rc<CursorManager> {
    Rc::new(CursorManager {
      output_manager: output_manager.clone(),
      window_manager,
      seat_manager: seat_manager.clone(),
      event_filter_manager,
      cursor,
      cursor_mgr,
      pointers: RefCell::new(vec![]),

      event_manager: RefCell::new(None),
    })
  }

  fn refresh_device_mappings(&self) {
    debug!("CursorManager::refresh_device_mappings");
    for pointer in self.pointers.borrow().iter() {
      if let Some(output_name) = pointer.output_name() {
        for output in self.output_manager.outputs().iter() {
          if output_name == output.name() {
            unsafe {
              wlr_cursor_map_input_to_output(self.cursor, pointer.raw_ptr(), output.raw_ptr());
            }
          }
        }
      }
    }
  }

  fn process_motion(&self, event: MotionEvent) {
    let position = event.position();

    unsafe {
      wlr_cursor_warp(self.cursor, event.raw_device(), position.x(), position.y());
    }

    let surface = self.window_manager.window_buffer_at(&position.into());

    if let Some(surface) = surface {
      if self.seat_manager.is_input_allowed(&surface) {
        let focus_changed = unsafe {
          (*self.seat_manager.raw_seat())
            .pointer_state
            .focused_surface
            != surface.wlr_surface()
        };
        let surface_position =
          position - FPoint::from(surface.buffer_extents().top_left()).as_displacement();

        // "Enter" the surface if necessary. This lets the client know that the
        // cursor has entered one of its surfaces.
        //
        // Note that this gives the surface "pointer focus", which is distinct
        // from cursor focus. You get pointer focus by moving the pointer over
        // a window.
        unsafe {
          wlr_seat_pointer_notify_enter(
            self.seat_manager.raw_seat(),
            surface.wlr_surface(),
            surface_position.x,
            surface_position.y,
          );
          if !focus_changed {
            // The enter event contains coordinates, so we only need to notify
            // on motion if the focus did not change.
            wlr_seat_pointer_notify_motion(
              self.seat_manager.raw_seat(),
              event.time_msec(),
              surface_position.x,
              surface_position.y,
            );
          }
        }
      }
    } else {
      unsafe {
        // If there's no surface under the cursor, set the cursor image to a
        // default. This is what makes the cursor image appear when you move it
        // around the screen, not over any surfaces.
        let cursor_image_name = CString::new("left_ptr").unwrap();
        wlr_xcursor_manager_set_cursor_image(
          self.cursor_mgr,
          cursor_image_name.as_ptr(),
          self.cursor,
        );
        // TODO: Change to wlr_seat_pointer_notify_clear_focus after updating wlroots
        wlr_seat_pointer_clear_focus(self.seat_manager.raw_seat());
      }
    }

    self
      .event_filter_manager
      .borrow_mut()
      .handle_pointer_motion_event(&event);
  }

  /// If there are any pointer device (mouse, touchpad, etc.) attached
  pub fn has_pointer_device(&self) -> bool {
    !self.pointers.borrow().is_empty()
  }

  /// Get the position of the cursor in global coordinates
  pub fn position(&self) -> FPoint {
    unsafe {
      FPoint {
        x: (*self.cursor).x,
        y: (*self.cursor).y,
      }
    }
  }

  pub fn raw_cursor(&self) -> *mut wlr_cursor {
    self.cursor
  }
}

pub(crate) trait CursorEventHandler {
  fn request_set_cursor(&self, event: *const wlr_seat_pointer_request_set_cursor_event);
  fn axis(&self, event: *const wlr_event_pointer_axis);
  fn button(&self, event: *const wlr_event_pointer_button);
  fn motion(&self, event: *const wlr_event_pointer_motion);
  fn motion_absolute(&self, event: *const wlr_event_pointer_motion_absolute);
  fn frame(&self);
}

impl CursorEventHandler for Rc<CursorManager> {
  fn request_set_cursor(&self, event: *const wlr_seat_pointer_request_set_cursor_event) {
    unsafe {
      // This event is rasied by the seat when a client provides a cursor image
      let focused_client = (*self.seat_manager.raw_seat()).pointer_state.focused_client;
      // This can be sent by any client, so we check to make sure this one is
      // actually has pointer focus first.
      if focused_client == (*event).seat_client {
        // Once we've vetted the client, we can tell the cursor to use the
        // provided surface as the cursor image. It will set the hardware cursor
        // on the output that it's currently on and continue to do so as the
        // cursor moves between outputs.
        wlr_cursor_set_surface(
          self.cursor,
          (*event).surface,
          (*event).hotspot_x,
          (*event).hotspot_y,
        );
      }
    }
  }

  fn axis(&self, event: *const wlr_event_pointer_axis) {
    let event = unsafe { AxisEvent::from_ptr(self.clone(), event) };

    let handled = self
      .event_filter_manager
      .borrow_mut()
      .handle_pointer_axis_event(&event);

    if !handled {
      unsafe {
        wlr_seat_pointer_notify_axis(
          self.seat_manager.raw_seat(),
          event.time_msec(),
          event.orientation(),
          event.delta(),
          event.delta_discrete(),
          event.source(),
        );
      }
    }
  }

  fn button(&self, event: *const wlr_event_pointer_button) {
    let event = unsafe { ButtonEvent::from_ptr(self.clone(), event) };

    let handled = self
      .event_filter_manager
      .borrow_mut()
      .handle_pointer_button_event(&event);

    if !handled {
      if event.state() == ButtonState::Pressed {
        let surface = self
          .window_manager
          .window_buffer_at(&self.position().into());

        if let Some(surface) = surface {
          if surface.can_receive_focus() {
            self.window_manager.focus_window(surface);
          }
        }
      }

      unsafe {
        wlr_seat_pointer_notify_button(
          self.seat_manager.raw_seat(),
          event.time_msec(),
          event.button(),
          event.state().as_raw(),
        );
      }
    }
  }

  // This event is forwarded by the cursor when a pointer emits a relative
  // pointer motion event (i.e. a delta)

  // The cursor doesn't move unless we tell it to. The cursor automatically
  // handles constraining the motion to the output layout, as well as any
  // special configuration applied for the specific input device which
  // generated the event. You can pass NULL for the device if you want to move
  // the cursor around without any input.
  fn motion(&self, event: *const wlr_event_pointer_motion) {
    let event = unsafe { RelativeMotionEvent::from_ptr(self.clone(), event) };

    self.process_motion(MotionEvent::Relative(event));
  }

  // This event is forwarded by the cursor when a pointer emits an absolute
  // motion event, from 0..1 on each axis. This happens, for example, when
  // wlroots is running under a Wayland window rather than KMS+DRM, and you
  // move the mouse over the window. You could enter the window from any edge,
  // so we have to warp the mouse there. There is also some hardware which
  // emits these events.
  fn motion_absolute(&self, event: *const wlr_event_pointer_motion_absolute) {
    let event = unsafe { AbsoluteMotionEvent::from_ptr(self.clone(), event) };

    self.process_motion(MotionEvent::Absolute(event));
  }

  fn frame(&self) {
    // This event is forwarded by the cursor when a pointer emits an frame
    // event. Frame events are sent after regular pointer events to group
    // multiple events together. For instance, two axis events may happen at the
    // same time, in which case a frame event won't be sent in between.
    // Notify the client with pointer focus of the frame event.
    unsafe {
      wlr_seat_pointer_notify_frame(self.seat_manager.raw_seat());
    }
  }
}

wayland_listener!(
  CursorEventManager,
  Rc<CursorManager>,
  [
    request_set_cursor => request_set_cursor_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let handler = &mut this.data;
      handler.request_set_cursor(data as _)
    };
    motion => motion_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let handler = &mut this.data;
      handler.motion(data as _)
    };
    motion_absolute => motion_absolute_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let handler = &mut this.data;
      handler.motion_absolute(data as _)
    };
    button => button_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let handler = &mut this.data;
      handler.button(data as _)
    };
    axis => axis_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let handler = &mut this.data;
      handler.axis(data as _)
    };
    frame => frame_func: |this: &mut CursorEventManager, _data: *mut libc::c_void,| unsafe {
      let handler = &mut this.data;
      handler.frame()
    };
  ]
);

#[cfg(test)]
mod tests {
  use super::*;
  use crate::input::seat::SeatManager;
  use crate::output_manager::OutputManager;
  use crate::{config::ConfigManager, test_util::*, window_management_policy::WmPolicyManager};
  use std::ptr;
  use std::rc::Rc;

  #[test]
  fn it_drops_and_cleans_up_on_destroy() {
    let config_manager = Rc::new(ConfigManager::default());
    let wm_policy_manager = Rc::new(WmPolicyManager::new());
    let seat_manager = SeatManager::mock(ptr::null_mut(), ptr::null_mut());
    let window_manager = Rc::new(WindowManager::init(
      wm_policy_manager.clone(),
      seat_manager.clone(),
      ptr::null_mut(),
    ));
    let output_manager =
      OutputManager::mock(config_manager, wm_policy_manager, window_manager.clone());
    let event_filter_manager = Rc::new(RefCell::new(EventFilterManager::new()));
    let cursor_manager = CursorManager::init(
      output_manager,
      window_manager.clone(),
      seat_manager.clone(),
      event_filter_manager,
      ptr::null_mut(),
    );

    let mut raw_pointer = wlr_pointer {
      impl_: ptr::null(),
      events: wlr_pointer__bindgen_ty_1 {
        motion: new_wl_signal(),
        motion_absolute: new_wl_signal(),
        button: new_wl_signal(),
        axis: new_wl_signal(),
        frame: new_wl_signal(),
        swipe_begin: new_wl_signal(),
        swipe_update: new_wl_signal(),
        swipe_end: new_wl_signal(),
        pinch_begin: new_wl_signal(),
        pinch_update: new_wl_signal(),
        pinch_end: new_wl_signal(),
      },
      data: ptr::null_mut(),
    };
    let mut device = wlr_input_device {
      impl_: ptr::null(),
      type_: wlr_input_device_type_WLR_INPUT_DEVICE_POINTER,
      vendor: 0,
      product: 0,
      name: ptr::null_mut(),
      width_mm: 0.0,
      height_mm: 0.0,
      output_name: ptr::null_mut(),
      __bindgen_anon_1: wlr_input_device__bindgen_ty_1 {
        pointer: &mut raw_pointer,
      },
      events: wlr_input_device__bindgen_ty_2 {
        destroy: new_wl_signal(),
      },
      data: ptr::null_mut(),
      link: new_wl_list(),
    };

    let destroy_signal = WlSignal::from_ptr(&mut device.events.destroy);

    let device = Device::init(&mut device);
    let weak_device = Rc::downgrade(&device);
    seat_manager.on_new_device.fire(device);
    let pointer = cursor_manager.pointers.borrow().first().unwrap().clone();

    let weak_pointer = Rc::downgrade(&pointer);
    drop(pointer);

    assert!(weak_device.upgrade().is_some());
    assert!(weak_pointer.upgrade().is_some());
    assert!(destroy_signal.listener_count() == 1);
    assert!(cursor_manager.has_pointer_device());

    destroy_signal.emit();

    assert!(weak_device.upgrade().is_none());
    assert!(weak_pointer.upgrade().is_none());
    assert!(destroy_signal.listener_count() == 0);
    assert!(!cursor_manager.has_pointer_device());
  }
}

#[cfg(test)]
unsafe fn wlr_cursor_attach_input_device(_: *mut wlr_cursor, _: *mut wlr_input_device) {}
#[cfg(test)]
unsafe fn wlr_cursor_map_input_to_output(
  _: *mut wlr_cursor,
  _: *mut wlr_input_device,
  _: *mut wlr_output,
) {
}
