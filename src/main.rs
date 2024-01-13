use ggez::conf::{WindowMode, WindowSetup};
use ggez::event::{self, EventHandler};
use ggez::glam::IVec2;
use ggez::graphics::{self, Canvas, Color, DrawParam, Image, Rect};
use ggez::input::keyboard::KeyInput;
use ggez::mint::Point2;
use ggez::winit::event::VirtualKeyCode;
use ggez::{Context, ContextBuilder, GameResult};

#[derive(Clone, Copy)]
enum Sprite {
	Player,
	Grass,
	Rock,
	Wall,
}

impl Sprite {
	fn rect_in_spritesheet(self) -> Rect {
		let (x, y) = match self {
			Sprite::Player => (0, 0),
			Sprite::Grass => (1, 1),
			Sprite::Rock => (3, 0),
			Sprite::Wall => (2, 0),
		};
		Rect::new(
			x as f32 * 8.0 / 128.0,
			y as f32 * 8.0 / 128.0,
			8.0 / 128.0,
			8.0 / 128.0,
		)
	}
}

fn draw_sprite(sprite: Sprite, dst: Rect, canvas: &mut Canvas, spritesheet: &Image) {
	canvas.draw(
		spritesheet,
		DrawParam::default()
			.dest_rect(dst)
			.src(sprite.rect_in_spritesheet()),
	);
}

enum ObjKind {
	Player,
	Rock,
	Wall,
}

struct Obj {
	kind: ObjKind,
	processed: bool,
	moved: bool,
}

impl Obj {
	fn can_move(&self) -> bool {
		match self.kind {
			ObjKind::Player => true,
			ObjKind::Rock => true,
			ObjKind::Wall => false,
		}
	}
}

struct Tile {
	obj: Option<Obj>,
}

impl Tile {
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
		grid.get_mut(Point2::from([3, 5])).unwrap().obj =
			Some(Obj { kind: ObjKind::Player, processed: false, moved: false });
		grid.get_mut(Point2::from([5, 5])).unwrap().obj =
			Some(Obj { kind: ObjKind::Rock, processed: false, moved: false });
		grid.get_mut(Point2::from([5, 4])).unwrap().obj =
			Some(Obj { kind: ObjKind::Rock, processed: false, moved: false });
		grid.get_mut(Point2::from([2, 2])).unwrap().obj =
			Some(Obj { kind: ObjKind::Wall, processed: false, moved: false });
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

	fn obj_move(&mut self, coords: Point2<i32>, direction: IVec2) {
		let coords_dst = IVec2::from(coords) + direction;
		let mut shall_move = false;
		if let Some(tile) = self.grid.get(coords) {
			if let Some(obj) = &tile.obj {
				if obj.can_move() {
					if let Some(tile_dst) = self.grid.get(coords_dst.into()) {
						if tile_dst.obj.is_some() {
							self.obj_move(coords_dst.into(), direction);
						}
					}
					if let Some(tile_dst) = self.grid.get(coords_dst.into()) {
						if tile_dst.obj.is_none() {
							shall_move = true;
						}
					}
				}
			}
		}
		if shall_move {
			let mut obj = self.grid.get_mut(coords).unwrap().obj.take();
			obj.as_mut().unwrap().moved = true;
			self.grid.get_mut(coords_dst.into()).unwrap().obj = obj;
		}
	}

	fn player_move(&mut self, direction: IVec2) {
		self.clear_processed_flags();
		self.clear_moved_flags();

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
						self.obj_move(coords, direction);
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
				let window_x = grid_x * 80;
				let window_y = grid_y * 80;
				draw_sprite(
					Sprite::Grass,
					Rect::new(window_x as f32, window_y as f32, 80.0 / 8.0, 80.0 / 8.0),
					&mut canvas,
					&self.spritesheet,
				);

				if let Some(obj) = &self.grid.get(Point2::from([grid_x, grid_y])).unwrap().obj {
					let sprite = match obj.kind {
						ObjKind::Player => Sprite::Player,
						ObjKind::Rock => Sprite::Rock,
						ObjKind::Wall => Sprite::Wall,
					};
					draw_sprite(
						sprite,
						Rect::new(window_x as f32, window_y as f32, 80.0 / 8.0, 80.0 / 8.0),
						&mut canvas,
						&self.spritesheet,
					);
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
		.window_mode(WindowMode::default().dimensions(960.0, 960.0))
		.build()
		.unwrap();
	let game = Game::new(&mut ctx)?;
	event::run(ctx, event_loop, game);
}
