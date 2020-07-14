#![macro_use]

/// Gets the offset of a field. Used by container_of!
macro_rules! offset_of(
  ($ty:ty, $field:ident) => {
    &(*(0 as *const $ty)).$field as *const _ as usize
  }
);

/// Gets the parent struct from a pointer.
/// VERY unsafe. The parent struct _must_ be repr(C), and the
/// type passed to this macro _must_ match the type of the parent.
macro_rules! container_of(
  ($ptr: expr, $container: ty, $field: ident) => {
    ($ptr as *mut u8).offset(-(offset_of!($container, $field) as isize)) as *mut $container
  }
);

/// Iterates over a wl_list.
///
/// # Safety
/// It is not safe to delete an element while iterating over the list,
/// so don't do it!
macro_rules! wl_list_for_each {
  ($ptr: expr, $field: ident, ($pos: ident : $container: ty) => $body: block) => {
    let mut $pos: *mut $container;
    $pos = container_of!($ptr.next, $container, $field);
    loop {
      if &(*$pos).$field as *const _ == &$ptr as *const _ {
        break;
      }
      {
        $body
      }
      $pos = container_of!((*$pos).$field.next, $container, $field);
    }
  };
}

/// Defines a new struct that contains a variable number of listeners that
/// will trigger unsafe user-defined callbacks.
///
/// The structure that is defined is repr(C), has one `data` field with the
/// given user type, and a field for each `$listener`.
///
/// Each `$listener` has a getter method that lets you get the pointer to the
/// listener. This method is unsafe, since it returns a raw pointer.
/// To use it correctly, you need to ensure that the data it refers to never
/// moves (e.g keep it in a box). The primary purpose of this method is to pass
/// the listener pointer to other methods to register it for a Wayland event.
/// **A listener can only be registered to one event at a time**.
///
/// Finally, it also takes in a body for each `$listener` that is called
/// every time the event that is later hooked up to it is fired.
/// This method is inherently unsafe, because the user data hasn't been cast
/// from the void pointer yet. It is the user's job to write this safely.
/// To highlight this fact, the body of the function must be prefixed with
/// `unsafe`.
///
/// # Example
/// ```rust,no_run,ignore
/// #[macro_use] extern crate wlroots;
/// extern crate wlroots_sys;
/// #[macro_use] extern crate wayland_sys;
/// extern crate libc;
///
/// use wlroots::InputDevice;
/// use wlroots_sys::wlr_input_device;
///
/// // Handles input addition and removal.
/// pub trait InputManagerHandler {
///     // Callback triggered when an input device is added.
///     fn input_added(&mut self, InputDevice);
/// }
///
/// wayland_listener!(
///     // The name of the structure that will be defined.
///     pub(crate) InputManager,
///     // The type that's stored in the `data` field.
///     // Note that we use a Box here to achieve dynamic dispatch,
///     // it's not required for this type to be in a box.
///     Box<InputManagerHandler>,
///     [
///         // Adds a new listener called `add_listener`.
///         // Adds an unsafe function called `add_notify` that is triggered
///         // whenever add_listener is activated from a Wayland event.
///         add_listener => add_notify: |this: &mut InputManager, data: *mut libc::c_void,| unsafe {
///             let ref mut manager = this.data;
///             // Call the method defined above, wrapping it in a safe interface.
///             // It is your job to ensure that the code in here doesn't trigger UB!
///             manager.input_added(InputDevice::from_ptr(data as *mut wlr_input_device))
///         };
///     ]
/// );
/// # fn main() {}
/// ```
///
/// # Unsafety
/// Note that the purpose of this macro is to make it easy to generate unsafe
/// boiler plate for using listeners with Rust data.
///
/// However, there are a few things this macro doesn't protect against.
///
/// First and foremost, the data cannot move. The listeners assume that the
/// structure will never move, so in order to defend against this the generated
/// `new` method returns a Box version. **Do not move out of the box**.
///
/// Second, this macro doesn't protect against the stored data being unsized.
/// Passing a pointer of unsized data to C is UB, don't do it.
macro_rules! wayland_listener {
  ($pub: vis $struct_name: ident, $data: ty, $([
      $($listener: ident => $listener_func: ident :
        |$($func_arg:ident: $func_type:ty,)*| unsafe $body: block;)*])+) => {
    #[repr(C)]
    $pub struct $struct_name {
        data: $data,
        $($($listener: Option<$crate::wayland_sys::server::wl_listener>),*)*
    }

    impl $struct_name {
      pub(crate) fn new(data: $data) -> ::std::pin::Pin<Box<$struct_name>> {
        ::std::pin::Pin::new(Box::new($struct_name {
          data,
          $($($listener: None),*)*
        }))
      }

      $($(#[cfg_attr(test, allow(dead_code))] pub(crate) unsafe extern "C" fn $listener(&mut self, signal: *mut $crate::wayland_sys::server::wl_signal) {
          if self.$listener.is_some() {
            self.$listener = None;
            panic!("Listener $listener is already bound");
          }
          self.$listener = Some({
            // NOTE Rationale for zeroed memory:
            // * Need to pass a pointer to wl_list_init
            // * The list is initialized by Wayland, which doesn't "drop"
            // * The listener is written to without dropping any of the data
            let mut listener: ::std::mem::MaybeUninit<$crate::wayland_sys::server::wl_listener> = ::std::mem::MaybeUninit::uninit();
            use $crate::wayland_sys::{ffi_dispatch, server::WAYLAND_SERVER_HANDLE};
            ffi_dispatch!(WAYLAND_SERVER_HANDLE,
                          wl_list_init,
                          &mut (*listener.as_mut_ptr()).link as *mut _ as _);
            (*listener.as_mut_ptr()).notify = $struct_name::$listener_func;
            listener.assume_init()
          });
          $crate::wayland_sys::server::signal::wl_signal_add(
            signal,
            self.$listener.as_ref().map_or_else(::std::ptr::null_mut, |x| x as *const _ as *mut _)
          );
      })*)*

      $($(#[cfg_attr(test, allow(dead_code))] pub(crate) unsafe extern "C" fn $listener_func(listener:
                                                *mut $crate::wayland_sys::server::wl_listener,
                                                data: *mut $crate::libc::c_void) {
        let manager: &mut $struct_name = &mut (*container_of!(listener,
                                                              $struct_name,
                                                              $listener));
        // TODO: Handle unwind
        // $crate::utils::handle_unwind(
        //     ::std::panic::catch_unwind(
        //         ::std::panic::AssertUnwindSafe(|| {
        //             #[allow(clippy::redundant_closure_call)]
        //             (|$($func_arg: $func_type,)*| { $body })(manager, data)
        //         })));
        #[allow(clippy::redundant_closure_call)]
        (|$($func_arg: $func_type,)*| { $body })(manager, data)
      })*)*
    }

    impl Drop for $struct_name {
      fn drop(&mut self) {
        unsafe {
          use $crate::wayland_sys::{ffi_dispatch, server::WAYLAND_SERVER_HANDLE};
          $($(
            if let Some(listener) = self.$listener.as_ref() {
              ffi_dispatch!(
                WAYLAND_SERVER_HANDLE,
                wl_list_remove,
                &listener.link as *const _ as *mut _
              );
            }
          )*)*
        }
      }
    }
  }
}

/// Makes moving clones into closures more convenient
#[macro_export]
macro_rules! listener {
    ($($n:ident),+ => move || $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            Box::new(move |_| $body)
        }
    );
    ($($n:ident),+ => move |$p:pat| $body:expr) => (
        {
            $( let $n = $n.clone(); )+
            Box::new(move |$p| $body)
        }
    );
}

#[cfg(test)]
mod tests {
  use crate::test_util::*;
  use wlroots_sys::libc;

  wayland_listener!(
    pub EventManager,
    u8,
    [
      map => map_func: |_this: &mut EventManager, _data: *mut libc::c_void,| unsafe {};
      unmap => unmap_func: |_this: &mut EventManager, _data: *mut libc::c_void,| unsafe {};
      destroy => destroy_func: |_this: &mut EventManager, _data: *mut libc::c_void,| unsafe {};
    ]
  );

  #[test]
  fn it_cleans_up_on_drop() {
    let mut event_manager = EventManager::new(0);

    let map_signal = WlSignal::new();
    let unmap_signal = WlSignal::new();
    let destroy_signal = WlSignal::new();

    unsafe {
      event_manager.map(map_signal.ptr());
      event_manager.unmap(unmap_signal.ptr());
      event_manager.destroy(destroy_signal.ptr());
    }

    assert!(map_signal.listener_count() == 1);
    assert!(unmap_signal.listener_count() == 1);
    assert!(destroy_signal.listener_count() == 1);

    drop(event_manager);

    assert!(map_signal.listener_count() == 0);
    assert!(unmap_signal.listener_count() == 0);
    assert!(destroy_signal.listener_count() == 0);
  }

  #[test]
  fn it_does_handle_not_beeing_bound_on_drop() {
    let mut event_manager = EventManager::new(0);

    let map_signal = WlSignal::new();
    let unmap_signal = WlSignal::new();
    let destroy_signal = WlSignal::new();

    unsafe {
      event_manager.map(map_signal.ptr());
    }

    assert!(map_signal.listener_count() == 1);
    assert!(unmap_signal.listener_count() == 0);
    assert!(destroy_signal.listener_count() == 0);

    drop(event_manager);

    assert!(map_signal.listener_count() == 0);
    assert!(unmap_signal.listener_count() == 0);
    assert!(destroy_signal.listener_count() == 0);
  }
}
