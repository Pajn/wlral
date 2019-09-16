use crate::geometry::{Displacement, Point, Rectangle, TransformMatrix};
use crate::surface::*;
use std::cell::RefCell;
use std::pin::Pin;
use std::ptr;
use std::rc::Rc;
use std::time::Instant;
use wayland_sys::server::signal::wl_signal_add;
use wlroots_sys::*;

pub trait OutputEventHandler {
  fn frame(&mut self);
}

impl OutputEventHandler for Output {
  fn frame(&mut self) {
    unsafe {
      // wlr_output_attach_render makes the OpenGL context current.
      if !wlr_output_attach_render(self.output, ptr::null_mut()) {
        return;
      }
      // The "effective" resolution can change if you rotate your outputs.
      let mut width: i32 = 0;
      let mut height: i32 = 0;
      wlr_output_effective_resolution(self.output, &mut width, &mut height);
      // Begin the renderer (calls glViewport and some other GL sanity checks)
      wlr_renderer_begin(self.renderer, width, height);

      let color = [0.3, 0.3, 0.3, 1.0];
      wlr_renderer_clear(self.renderer, &color[0]);

      let now = Instant::now();
      let since_creation = now.duration_since(self.created_at);
      let frame_time = timespec {
        tv_sec: since_creation.as_secs() as i64,
        tv_nsec: since_creation.subsec_nanos() as i64,
      };

      for surface in self.surface_manager.borrow().surfaces_to_render() {
        self.render_surface(&frame_time, surface);
      }

      // Hardware cursors are rendered by the GPU on a separate plane, and can be
      // moved around without re-rendering what's beneath them - which is more
      // efficient. However, not all hardware supports hardware cursors. For this
      // reason, wlroots provides a software fallback, which we ask it to render
      // here. wlr_cursor handles configuring hardware vs software cursors for you,
      // and this function is a no-op when hardware cursors are in use.
      wlr_output_render_software_cursors(self.output, ptr::null_mut());

      // Conclude rendering and swap the buffers, showing the final frame
      // on-screen.
      wlr_renderer_end(self.renderer);
      wlr_output_commit(self.output);
    }
  }
}

wayland_listener!(
  pub OutputEventManager,
  Rc<RefCell<dyn OutputEventHandler>>,
  [
     frame => frame_func: |this: &mut OutputEventManager, _data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.borrow_mut().frame()
     };
  ]
);

pub struct Output {
  renderer: *mut wlr_renderer,
  surface_manager: Rc<RefCell<SurfaceManager>>,
  output_layout: *mut wlr_output_layout,
  output: *mut wlr_output,
  created_at: Instant,

  event_manager: Option<Pin<Box<OutputEventManager>>>,
}

impl Output {
  pub fn from_ptr(
    renderer: *mut wlr_renderer,
    surface_manager: Rc<RefCell<SurfaceManager>>,
    output_layout: *mut wlr_output_layout,
    output: *mut wlr_output,
  ) -> Output {
    Output {
      renderer,
      surface_manager,
      output,
      output_layout,
      created_at: Instant::now(),
      event_manager: None,
    }
  }

  pub fn use_preferred_mode(&self) {
    unsafe {
      // Some backends don't have modes. DRM+KMS does, and we need to set a mode
      // before we can use the output. The mode is a tuple of (width, height,
      // refresh rate), and each monitor supports only a specific set of modes. We
      // just pick the first, a more sophisticated compositor would let the user
      // configure it or pick the mode the display advertises as preferred.
      let length = ffi_dispatch!(WAYLAND_SERVER_HANDLE, wl_list_length, &(*self.output).modes);
      if length > 0 {
        let mode: *mut wlr_output_mode =
          container_of!((*self.output).modes.prev, wlr_output_mode, link);
        wlr_output_set_mode(self.output, mode);
      }
    }
  }

  pub fn top_left(&self) -> Point {
    unsafe {
      let mut x = 0.0;
      let mut y = 0.0;
      wlr_output_layout_output_coords(self.output_layout, self.output, &mut x, &mut y);
      Point {
        x: x as i32,
        y: y as i32,
      }
    }
  }

  pub fn scale(&self) -> f32 {
    unsafe { (*self.output).scale }
  }

  pub fn transform_matrix(&self) -> TransformMatrix {
    unsafe { TransformMatrix((*self.output).transform_matrix) }
  }

  pub fn render_surface(&self, frame_time: &timespec, surface: Rc<Surface>) {
    unsafe {
      let wlr_surface = &mut *surface.surface();

      // We first obtain a wlr_texture, which is a GPU resource. wlroots
      // automatically handles negotiating these with the client. The underlying
      // resource could be an opaque handle passed from the client, or the client
      // could have sent a pixel buffer which we copied to the GPU, or a few other
      // means. You don't have to worry about this, wlroots takes care of it.
      let texture = wlr_surface_get_texture(wlr_surface);
      if texture.is_null() {
        return;
      }

      // The view has a position in layout coordinates. If you have two displays,
      // one next to the other, both 1080p, a view on the rightmost display might
      // have layout coordinates of 2000,100. We need to translate that to
      // output-local coordinates, or (2000 - 1920).
      // double ox = 0, oy = 0;
      // wlr_output_layout_output_coords(
      //     view->server->output_layout, output, &ox, &oy);
      // ox += view->x + sx, oy += view->y + sy;
      let top_left = self.top_left()
        + surface.buffer_top_left().as_displacement()
        + Displacement {
          dx: wlr_surface.sx,
          dy: wlr_surface.sy,
        };

      // We also have to apply the scale factor for HiDPI outputs. This is only
      // part of the puzzle, TinyWL does not fully support HiDPI.
      let render_box = Rectangle {
        top_left: top_left * self.scale(),
        size: surface.buffer_size() * self.scale(),
      }
      .into();

      // Those familiar with OpenGL are also familiar with the role of matricies
      // in graphics programming. We need to prepare a matrix to render the view
      // with. wlr_matrix_project_box is a helper which takes a box with a desired
      // x, y coordinates, width and height, and an output geometry, then
      // prepares an orthographic projection and multiplies the necessary
      // transforms to produce a model-view-projection matrix.
      //
      // Naturally you can do this any way you like, for example to make a 3D
      // compositor.
      let transform = wlr_output_transform_invert(wlr_surface.current.transform);
      let rotation = 0.0;
      let mut matrix = TransformMatrix::IDENTITY.clone();
      wlr_matrix_project_box(
        matrix.as_mut_ptr(),
        &render_box,
        transform,
        rotation,
        self.transform_matrix().as_ptr(),
      );

      // This takes our matrix, the texture, and an alpha, and performs the actual
      // rendering on the GPU.
      let alpha = 1.0;
      wlr_render_texture_with_matrix(self.renderer, texture, matrix.as_ptr(), alpha);

      // This lets the client know that we've displayed that frame and it can
      // prepare another one now if it likes.
      wlr_surface_send_frame_done(wlr_surface, frame_time);
    }
  }
}

pub trait OutputEvents {
  fn bind_events(&self);
}

impl OutputEvents for Rc<RefCell<Output>> {
  fn bind_events(&self) {
    let mut event_manager = OutputEventManager::new(self.clone());

    unsafe {
      wl_signal_add(
        &mut (*self.borrow().output).events.frame,
        event_manager.frame(),
      );
    }

    self.borrow_mut().event_manager = Some(event_manager);
  }
}

pub struct OutputManagerEventHandler {
  renderer: *mut wlr_renderer,
  surface_manager: Rc<RefCell<SurfaceManager>>,
  output_layout: *mut wlr_output_layout,
  outputs: Vec<Rc<RefCell<Output>>>,
}

impl OutputManagerEventHandler {
  fn new_output(&mut self, output: Output) {
    println!("new_output");

    output.use_preferred_mode();

    unsafe {
      // Adds this to the output layout. The add_auto function arranges outputs
      // from left-to-right in the order they appear. A more sophisticated
      // compositor would let the user configure the arrangement of outputs in the
      // layout.
      wlr_output_layout_add_auto(self.output_layout, output.output);

      // Creating the global adds a wl_output global to the display, which Wayland
      // clients can see to find out information about the output (such as
      // DPI, scale factor, manufacturer, etc).
      wlr_output_create_global(output.output);
    }

    let output = Rc::new(RefCell::new(output));

    output.bind_events();

    self.outputs.push(output);
  }
}

wayland_listener!(
  pub OutputManagerEventManager,
  Rc<RefCell<OutputManagerEventHandler>>,
  [
     new_output => new_output_func: |this: &mut OutputManagerEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         let renderer = handler.borrow().renderer;
         let surface_manager = handler.borrow().surface_manager.clone();
         let output_layout = handler.borrow().output_layout;
         handler.borrow_mut().new_output(Output::from_ptr(renderer, surface_manager, output_layout, data as *mut wlr_output))
     };
  ]
);

#[allow(unused)]
pub struct OutputManager {
  event_manager: Pin<Box<OutputManagerEventManager>>,
  event_handler: Rc<RefCell<OutputManagerEventHandler>>,
}

impl OutputManager {
  pub fn init(
    backend: *mut wlr_backend,
    renderer: *mut wlr_renderer,
    surface_manager: Rc<RefCell<SurfaceManager>>,
    output_layout: *mut wlr_output_layout,
  ) -> OutputManager {
    println!("OutputManager::init prebind");

    let event_handler = Rc::new(RefCell::new(OutputManagerEventHandler {
      renderer,
      surface_manager,
      output_layout,
      outputs: vec![],
    }));

    let mut event_manager = OutputManagerEventManager::new(event_handler.clone());
    unsafe {
      wl_signal_add(
        &mut (*backend).events.new_output,
        event_manager.new_output(),
      );
    }

    println!("OutputManager::init postbind");

    OutputManager {
      event_manager,
      event_handler,
    }
  }
}
