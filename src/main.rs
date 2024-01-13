use std::time::{Duration, Instant};

use ggez::conf::{WindowMode, WindowSetup};
use ggez::event::{self, EventHandler};
use ggez::glam::IVec2;
use ggez::graphics::{self, Canvas, Color, DrawParam, Image, Rect};
use ggez::input::keyboard::KeyInput;
use ggez::mint::Point2;
use ggez::winit::event::VirtualKeyCode;
use ggez::{Context, ContextBuilder, GameResult};

fn tile_rect(coords: Point2<i32>) -> Rect {
	Rect::new(
		coords.x as f32 * Tile::W,
		coords.y as f32 * Tile::H,
		Tile::W / 8.0,
		Tile::H / 8.0,
	)
}

fn lerp(progress: f32, start: f32, end: f32) -> f32 {
	start + progress * (end - start)
}

#[derive(Clone, Copy)]
enum Sprite {
	Player,
	Grass,
	Rock,
	Wall,
	Rope,
}

impl Sprite {
	fn rect_in_spritesheet(self) -> Rect {
		let (x, y) = match self {
			Sprite::Player => (0, 0),
			Sprite::Grass => (1, 1),
			Sprite::Rock => (3, 0),
			Sprite::Wall => (2, 0),
			Sprite::Rope => (4, 0),
		};
		Rect::new(
			x as f32 * 8.0 / 128.0,
			y as f32 * 8.0 / 128.0,
			8.0 / 128.0,
			8.0 / 128.0,
		)
	}
}

fn draw_sprite(sprite: Sprite, dst: Rect, z: i32, canvas: &mut Canvas, spritesheet: &Image) {
	canvas.draw(
		spritesheet,
		DrawParam::default()
			.dest_rect(dst)
			.src(sprite.rect_in_spritesheet())
			.z(z),
	);
}

enum Animation {
	None,
	CommingFrom {
		src: Point2<i32>,
		time_start: Instant,
		duration: Duration,
	},
	FailingToMoveTo {
		dst: Point2<i32>,
		time_start: Instant,
		duration: Duration,
	},
}

enum ObjKind {
	Player,
	Rock,
	Wall,
	Rope,
}

struct Obj {
	kind: ObjKind,
	processed: bool,
	moved: bool,
	animation: Animation,
}

impl Obj {
	fn from_kind(kind: ObjKind) -> Obj {
		Obj { kind, processed: false, moved: false, animation: Animation::None }
	}

	fn can_move(&self) -> bool {
		match self.kind {
			ObjKind::Player => true,
			ObjKind::Rock => true,
			ObjKind::Wall => false,
			ObjKind::Rope => true,
		}
	}
}

struct Tile {
	obj: Option<Obj>,
}

impl Tile {
	const W: f32 = 80.0;
	const H: f32 = 80.0;

	fn new() -> Tile {
		Tile { obj: None }
	}
}

struct Grid {
	tiles: Vec<Tile>,
}

impl Grid {
	const W: i32 = 12;
	const H: i32 = 12;

	fn new() -> Grid {
		let mut tiles = vec![];
		for _i in 0..(Grid::W * Grid::H) {
			tiles.push(Tile::new());
		}
		Grid { tiles }
	}

	fn index(&self, coords: Point2<i32>) -> Option<usize> {
		if 0 <= coords.x && coords.x < Grid::W && 0 <= coords.y && coords.y < Grid::H {
			let index = (coords.y * Grid::W + coords.x) as usize;
			assert!(index < self.tiles.len());
			Some(index)
		} else {
			None
		}
	}

	fn get(&self, coords: Point2<i32>) -> Option<&Tile> {
		let index = self.index(coords)?;
		self.tiles.get(index)
	}
	fn get_mut(&mut self, coords: Point2<i32>) -> Option<&mut Tile> {
		let index = self.index(coords)?;
		self.tiles.get_mut(index)
	}
}

struct Game {
	grid: Grid,
	spritesheet: Image,
}

impl Game {
	pub fn new(ctx: &mut Context) -> GameResult<Game> {
		let mut grid = Grid::new();
		grid.get_mut(Point2::from([3, 5])).unwrap().obj = Some(Obj::from_kind(ObjKind::Player));
		grid.get_mut(Point2::from([2, 5])).unwrap().obj = Some(Obj::from_kind(ObjKind::Player));
		grid.get_mut(Point2::from([5, 4])).unwrap().obj = Some(Obj::from_kind(ObjKind::Rock));
		grid.get_mut(Point2::from([5, 5])).unwrap().obj = Some(Obj::from_kind(ObjKind::Rock));
		grid.get_mut(Point2::from([5, 6])).unwrap().obj = Some(Obj::from_kind(ObjKind::Rope));
		grid.get_mut(Point2::from([5, 7])).unwrap().obj = Some(Obj::from_kind(ObjKind::Rope));
		grid.get_mut(Point2::from([2, 2])).unwrap().obj = Some(Obj::from_kind(ObjKind::Wall));
		Ok(Game {
			grid,
			spritesheet: Image::from_bytes(ctx, include_bytes!("../assets/spritesheet.png"))?,
		})
	}

	fn clear_processed_flags(&mut self) {
		for tile in self.grid.tiles.iter_mut() {
			if let Some(obj) = &mut tile.obj {
				obj.processed = false;
			}
		}
	}
	fn clear_moved_flags(&mut self) {
		for tile in self.grid.tiles.iter_mut() {
			if let Some(obj) = &mut tile.obj {
				obj.moved = false;
			}
		}
	}
	fn clear_animations(&mut self) {
		for tile in self.grid.tiles.iter_mut() {
			if let Some(obj) = &mut tile.obj {
				obj.animation = Animation::None;
			}
		}
	}

	fn obj_move(&mut self, coords: Point2<i32>, direction: IVec2, pushed: bool) {
		let coords_dst = IVec2::from(coords) + direction;
		let mut shall_move = false;
		let mut failed_to_move = false;
		if let Some(tile) = self.grid.get(coords) {
			if let Some(obj) = &tile.obj {
				if obj.can_move() {
					if let Some(tile_dst) = self.grid.get(coords_dst.into()) {
						if tile_dst.obj.is_some() {
							self.obj_move(coords_dst.into(), direction, true);
						}
					}
					if let Some(tile_dst) = self.grid.get(coords_dst.into()) {
						if tile_dst.obj.is_none() {
							shall_move = true;
						} else {
							failed_to_move = true;
						}
					}
				}
			}
		}

		let mut obj_is_rope = false;
		if shall_move {
			let mut obj = self.grid.get_mut(coords).unwrap().obj.take();
			obj.as_mut().unwrap().moved = true;
			obj.as_mut().unwrap().animation = Animation::CommingFrom {
				src: coords,
				time_start: Instant::now(),
				duration: Duration::from_secs_f32(0.05),
			};
			obj_is_rope = matches!(obj.as_mut().unwrap().kind, ObjKind::Rope);
			self.grid.get_mut(coords_dst.into()).unwrap().obj = obj;
		} else if failed_to_move {
			self
				.grid
				.get_mut(coords)
				.unwrap()
				.obj
				.as_mut()
				.unwrap()
				.animation = Animation::FailingToMoveTo {
				dst: coords_dst.into(),
				time_start: Instant::now(),
				duration: Duration::from_secs_f32(0.05),
			};
		}

		if shall_move && !pushed {
			let coords_maybe_pulled = IVec2::from(coords) - direction;
			if obj_is_rope
				|| self
					.grid
					.get(coords_maybe_pulled.into())
					.is_some_and(|tile| {
						tile
							.obj
							.as_ref()
							.is_some_and(|obj| matches!(obj.kind, ObjKind::Rope))
					}) {
				self.obj_move(coords_maybe_pulled.into(), direction, false);
			}
		}
	}

	fn player_move(&mut self, direction: IVec2) {
		self.clear_processed_flags();
		self.clear_moved_flags();
		self.clear_animations();

		for grid_y in 0..Grid::H {
			for grid_x in 0..Grid::W {
				let coords = Point2::from([grid_x, grid_y]);
				if let Some(obj) = &self.grid.get(coords).unwrap().obj {
					if matches!(obj.kind, ObjKind::Player) && !obj.processed && !obj.moved {
						self
							.grid
							.get_mut(coords)
							.unwrap()
							.obj
							.as_mut()
							.unwrap()
							.processed = true;
						self.obj_move(coords, direction, false);
					}
				}
			}
		}
	}
}

impl EventHandler for Game {
	fn update(&mut self, _ctx: &mut Context) -> GameResult {
		Ok(())
	}

	fn key_down_event(&mut self, ctx: &mut Context, input: KeyInput, _repeated: bool) -> GameResult {
		match input.keycode {
			Some(VirtualKeyCode::Escape) => ctx.request_quit(),
			Some(VirtualKeyCode::Up) => self.player_move(IVec2::from([0, -1])),
			Some(VirtualKeyCode::Down) => self.player_move(IVec2::from([0, 1])),
			Some(VirtualKeyCode::Left) => self.player_move(IVec2::from([-1, 0])),
			Some(VirtualKeyCode::Right) => self.player_move(IVec2::from([1, 0])),
			_ => {},
		}

		Ok(())
	}

	fn draw(&mut self, ctx: &mut Context) -> GameResult {
		let mut canvas = Canvas::from_frame(ctx, Color::BLACK);
		canvas.set_sampler(graphics::Sampler::nearest_clamp());

		for grid_y in 0..Grid::H {
			for grid_x in 0..Grid::W {
				let coords = Point2::from([grid_x, grid_y]);

				draw_sprite(
					Sprite::Grass,
					tile_rect(coords),
					1,
					&mut canvas,
					&self.spritesheet,
				);

				if let Some(obj) = &self.grid.get(Point2::from([grid_x, grid_y])).unwrap().obj {
					let sprite = match obj.kind {
						ObjKind::Player => Sprite::Player,
						ObjKind::Rock => Sprite::Rock,
						ObjKind::Wall => Sprite::Wall,
						ObjKind::Rope => Sprite::Rope,
					};
					let rect = match obj.animation {
						Animation::None => tile_rect(coords),
						Animation::CommingFrom { src, time_start, duration } => {
							let src_rect = tile_rect(src);
							let dst_rect = tile_rect(coords);
							let progress = time_start.elapsed().as_secs_f32() / duration.as_secs_f32();
							let progress = progress.clamp(0.0, 1.0);
							let window_x = lerp(progress, src_rect.x, dst_rect.x);
							let window_y = lerp(progress, src_rect.y, dst_rect.y);
							Rect::new(window_x, window_y, dst_rect.w, dst_rect.h)
						},
						Animation::FailingToMoveTo { dst, time_start, duration } => {
							let src_rect = tile_rect(coords);
							let mut dst_rect = tile_rect(dst);
							dst_rect.x = lerp(0.1, src_rect.x, dst_rect.x);
							dst_rect.y = lerp(0.1, src_rect.y, dst_rect.y);
							let progress = time_start.elapsed().as_secs_f32() / duration.as_secs_f32();
							let progress = progress.clamp(0.0, 1.0);
							let (window_x, window_y) = if progress <= 0.5 {
								let window_x = lerp(progress * 2.0, src_rect.x, dst_rect.x);
								let window_y = lerp(progress * 2.0, src_rect.y, dst_rect.y);
								(window_x, window_y)
							} else {
								let window_x = lerp(progress * 2.0 - 1.0, dst_rect.x, src_rect.x);
								let window_y = lerp(progress * 2.0 - 1.0, dst_rect.y, src_rect.y);
								(window_x, window_y)
							};
							Rect::new(window_x, window_y, dst_rect.w, dst_rect.h)
						},
					};
					draw_sprite(sprite, rect, 2, &mut canvas, &self.spritesheet);
				}
			}
		}

		canvas.finish(ctx)?;
		Ok(())
	}
}

fn main() -> GameResult {
	let (mut ctx, event_loop) = ContextBuilder::new("Puzh", "Anima :3")
		.window_setup(WindowSetup::default().title("Puzh").vsync(true).srgb(false))
		.window_mode(
			WindowMode::default().dimensions(Grid::W as f32 * Tile::W, Grid::H as f32 * Tile::H),
		)
		.build()
		.unwrap();
	let game = Game::new(&mut ctx)?;
	event::run(ctx, event_loop, game);
}
