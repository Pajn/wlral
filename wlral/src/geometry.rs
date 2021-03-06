use std::ops::{Add, Div, Mul, Sub};
use wlroots_sys::wlr_box;

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct TPoint<T: Copy> {
  pub x: T,
  pub y: T,
}

pub type Point = TPoint<i32>;
pub type FPoint = TPoint<f64>;

impl<T: Copy> TPoint<T> {
  pub fn x(&self) -> T {
    self.x
  }

  pub fn y(&self) -> T {
    self.y
  }

  pub fn as_displacement(&self) -> TDisplacement<T> {
    TDisplacement {
      dx: self.x,
      dy: self.y,
    }
  }
}

impl Point {
  pub const ZERO: Point = Point { x: 0, y: 0 };
}

impl FPoint {
  pub const ZERO: FPoint = FPoint { x: 0.0, y: 0.0 };
}

impl From<Point> for FPoint {
  fn from(point: Point) -> Self {
    FPoint {
      x: point.x as f64,
      y: point.y as f64,
    }
  }
}

impl From<FPoint> for Point {
  fn from(point: FPoint) -> Self {
    Point {
      x: point.x as i32,
      y: point.y as i32,
    }
  }
}

impl Mul<f32> for Point {
  type Output = Point;

  fn mul(self, other: f32) -> Point {
    Point {
      x: ((self.x as f32) * other) as i32,
      y: ((self.y as f32) * other) as i32,
    }
  }
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct Size {
  pub width: i32,
  pub height: i32,
}

impl Size {
  pub const ZERO: Size = Size {
    width: 0,
    height: 0,
  };

  pub fn width(&self) -> i32 {
    self.width
  }

  pub fn height(&self) -> i32 {
    self.height
  }

  pub fn with_width(&self, width: i32) -> Size {
    Size {
      width,
      height: self.height,
    }
  }

  pub fn with_height(&self, height: i32) -> Size {
    Size {
      width: self.width,
      height,
    }
  }

  pub fn as_displacement(&self) -> Displacement {
    TDisplacement {
      dx: self.width,
      dy: self.height,
    }
  }
}

impl Add<Size> for Size {
  type Output = Size;

  fn add(self, other: Size) -> Self::Output {
    Size {
      width: self.width + other.width,
      height: self.height + other.height,
    }
  }
}

impl Sub<Size> for Size {
  type Output = Size;

  fn sub(self, other: Size) -> Self::Output {
    Size {
      width: self.width - other.width,
      height: self.height - other.height,
    }
  }
}

impl Mul<f32> for Size {
  type Output = Size;

  fn mul(self, other: f32) -> Size {
    Size {
      width: ((self.width as f32) * other) as i32,
      height: ((self.height as f32) * other) as i32,
    }
  }
}

impl Div<f32> for Size {
  type Output = Size;

  fn div(self, other: f32) -> Size {
    Size {
      width: ((self.width as f32) / other) as i32,
      height: ((self.height as f32) / other) as i32,
    }
  }
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct Rectangle {
  pub top_left: Point,
  pub size: Size,
}

impl Rectangle {
  pub const ZERO: Rectangle = Rectangle {
    top_left: Point::ZERO,
    size: Size::ZERO,
  };

  pub fn left(&self) -> i32 {
    self.top_left.x
  }

  pub fn top(&self) -> i32 {
    self.top_left.y
  }

  pub fn right(&self) -> i32 {
    self.left() + self.width()
  }

  pub fn bottom(&self) -> i32 {
    self.top() + self.height()
  }

  pub fn center_x(&self) -> i32 {
    self.left() + self.width() / 2
  }

  pub fn center_y(&self) -> i32 {
    self.top() + self.height() / 2
  }

  pub fn width(&self) -> i32 {
    self.size.width
  }

  pub fn height(&self) -> i32 {
    self.size.height
  }

  pub fn top_left(&self) -> Point {
    self.top_left
  }

  pub fn bottom_right(&self) -> Point {
    Point {
      x: self.right(),
      y: self.bottom(),
    }
  }

  pub fn size(&self) -> Size {
    self.size
  }

  pub fn contains(&self, point: &Point) -> bool {
    self.left() <= point.x
      && self.right() > point.x
      && self.top() <= point.y
      && self.bottom() > point.y
  }

  pub fn overlaps(&self, rectangle: &Rectangle) -> bool {
    let disjoint = rectangle.left() >= self.right()
      || rectangle.right() <= self.left()
      || rectangle.top() >= self.bottom()
      || rectangle.bottom() <= self.top()
      || self.width() == 0
      || self.height() == 0
      || rectangle.width() == 0
      || rectangle.height() == 0;

    !disjoint
  }
}

impl From<wlr_box> for Rectangle {
  fn from(wlr_box: wlr_box) -> Self {
    Rectangle {
      top_left: Point {
        x: wlr_box.x,
        y: wlr_box.y,
      },
      size: Size {
        width: wlr_box.width,
        height: wlr_box.height,
      },
    }
  }
}

impl From<Rectangle> for wlr_box {
  fn from(rect: Rectangle) -> Self {
    wlr_box {
      x: rect.top_left.x,
      y: rect.top_left.y,
      width: rect.size.width,
      height: rect.size.height,
    }
  }
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct TDisplacement<T: Copy> {
  pub dx: T,
  pub dy: T,
}

impl<T: Copy> TDisplacement<T> {
  pub fn delta_x(&self) -> T {
    self.dx
  }

  pub fn delta_y(&self) -> T {
    self.dy
  }
}

pub type Displacement = TDisplacement<i32>;
pub type FDisplacement = TDisplacement<f64>;

impl Displacement {
  pub const ZERO: Displacement = Displacement { dx: 0, dy: 0 };

  pub fn length(&self) -> f64 {
    ((self.dx.pow(2) + self.dy.pow(2)) as f64).sqrt()
  }
}

impl FDisplacement {
  pub const ZERO: FDisplacement = FDisplacement { dx: 0.0, dy: 0.0 };

  pub fn length(&self) -> f64 {
    ((self.dx.powi(2) + self.dy.powi(2)) as f64).sqrt()
  }
}

impl From<Displacement> for FDisplacement {
  fn from(point: Displacement) -> Self {
    FDisplacement {
      dx: point.dx as f64,
      dy: point.dy as f64,
    }
  }
}

impl From<FDisplacement> for Displacement {
  fn from(point: FDisplacement) -> Self {
    Displacement {
      dx: point.dx as i32,
      dy: point.dy as i32,
    }
  }
}

impl<T: Copy> Add<TDisplacement<T>> for TDisplacement<T>
where
  T: Add<T, Output = T>,
{
  type Output = TDisplacement<T>;

  fn add(self, other: TDisplacement<T>) -> Self::Output {
    TDisplacement {
      dx: self.dx + other.dx,
      dy: self.dy + other.dy,
    }
  }
}

impl<T: Copy> Sub<TDisplacement<T>> for TDisplacement<T>
where
  T: Sub<T, Output = T>,
{
  type Output = TDisplacement<T>;

  fn sub(self, other: TDisplacement<T>) -> Self::Output {
    TDisplacement {
      dx: self.dx - other.dx,
      dy: self.dy - other.dy,
    }
  }
}

impl<T: Copy> Sub<TPoint<T>> for TPoint<T>
where
  T: Sub<T, Output = T>,
{
  type Output = TDisplacement<T>;

  fn sub(self, other: Self) -> Self::Output {
    TDisplacement {
      dx: self.x - other.x,
      dy: self.y - other.y,
    }
  }
}

impl<T: Copy> Add<TDisplacement<T>> for TPoint<T>
where
  T: Add<T, Output = T>,
{
  type Output = TPoint<T>;

  fn add(self, other: TDisplacement<T>) -> Self::Output {
    TPoint {
      x: self.x + other.dx,
      y: self.y + other.dy,
    }
  }
}

impl<T: Copy> Sub<TDisplacement<T>> for TPoint<T>
where
  T: Sub<T, Output = T>,
{
  type Output = TPoint<T>;

  fn sub(self, other: TDisplacement<T>) -> Self::Output {
    TPoint {
      x: self.x - other.dx,
      y: self.y - other.dy,
    }
  }
}

impl Add<Displacement> for Rectangle {
  type Output = Rectangle;

  fn add(self, other: Displacement) -> Self::Output {
    Rectangle {
      top_left: self.top_left + other,
      size: self.size,
    }
  }
}

impl Sub<Displacement> for Rectangle {
  type Output = Rectangle;

  fn sub(self, other: Displacement) -> Self::Output {
    Rectangle {
      top_left: self.top_left - other,
      size: self.size,
    }
  }
}

#[repr(transparent)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct TransformMatrix(pub [f32; 9]);

#[rustfmt::skip]
impl TransformMatrix {
  pub const IDENTITY: TransformMatrix =
    TransformMatrix([
      1.0, 0.0, 0.0, 
      0.0, 1.0, 0.0, 
      0.0, 0.0, 1.0
    ]);

  pub fn translate(x: f32, y: f32) -> TransformMatrix {
    TransformMatrix([
      1.0, 0.0, x, 
      0.0, 1.0, y, 
      0.0, 0.0, 1.0
    ])
  }
  pub fn scale(x: f32, y: f32) -> TransformMatrix {
    TransformMatrix([
    x,   0.0, 0.0, 
      0.0, y,   0.0, 
      0.0, 0.0, 1.0
    ])
  }

  pub fn as_ptr(&self) -> *const f32 {
    self.0.as_ptr()
  }

  pub fn as_mut_ptr(&mut self) -> *mut f32 {
    self.0.as_mut_ptr()
  }
}

impl Mul for TransformMatrix {
  type Output = TransformMatrix;

  #[rustfmt::skip]
  fn mul(self, rhs: Self) -> Self::Output {
    let a = self.0;
    let b = rhs.0;
    TransformMatrix([
      a[0]*b[0] + a[1]*b[3] + a[2]*b[6],
      a[0]*b[1] + a[1]*b[4] + a[2]*b[7],
      a[0]*b[2] + a[1]*b[5] + a[2]*b[8],

      a[3]*b[0] + a[4]*b[3] + a[5]*b[6],
      a[3]*b[1] + a[4]*b[4] + a[5]*b[7],
      a[3]*b[2] + a[4]*b[5] + a[5]*b[8],

      a[6]*b[0] + a[7]*b[3] + a[8]*b[6],
      a[6]*b[1] + a[7]*b[4] + a[8]*b[7],
      a[6]*b[2] + a[7]*b[5] + a[8]*b[8],
    ])
  }
}

#[cfg(test)]
mod test {
  use super::*;

  #[test]
  fn test_rectangle_overlaps() {
    let rect1 = Rectangle {
      top_left: Point { x: 0, y: 0 },
      size: Size {
        width: 1,
        height: 1,
      },
    };
    let rect2 = Rectangle {
      top_left: Point { x: 1, y: 1 },
      size: Size {
        width: 1,
        height: 1,
      },
    };
    let rect3 = Rectangle {
      top_left: Point { x: 0, y: 0 },
      size: Size {
        width: 2,
        height: 2,
      },
    };
    let rect4 = Rectangle {
      top_left: Point { x: -1, y: -1 },
      size: Size {
        width: 2,
        height: 2,
      },
    };
    let rect_empty = Rectangle {
      top_left: Point { x: 0, y: 0 },
      size: Size {
        width: 0,
        height: 0,
      },
    };

    assert!(!rect_empty.overlaps(&rect_empty));
    assert!(!rect_empty.overlaps(&rect1));
    assert!(!rect_empty.overlaps(&rect4));

    assert!(!rect1.overlaps(&rect2));
    assert!(!rect2.overlaps(&rect1));
    assert!(!rect4.overlaps(&rect2));
    assert!(!rect2.overlaps(&rect4));

    assert!(rect1.overlaps(&rect1));
    assert!(rect4.overlaps(&rect4));

    assert!(rect3.overlaps(&rect1));
    assert!(rect1.overlaps(&rect3));
    assert!(rect3.overlaps(&rect2));
    assert!(rect2.overlaps(&rect3));

    assert!(rect4.overlaps(&rect1));
    assert!(rect1.overlaps(&rect4));
    assert!(rect4.overlaps(&rect3));
    assert!(rect3.overlaps(&rect3));
  }
}
