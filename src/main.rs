use std::collections::HashMap;
use std::time::{Duration, Instant};

use ggez::conf::{WindowMode, WindowSetup};
use ggez::event::{self, EventHandler};
use ggez::glam::{IVec2, Vec2};
use ggez::graphics::{self, Canvas, Color, DrawParam, Image, Rect};
use ggez::input::keyboard::KeyInput;
use ggez::mint::Point2;
use ggez::winit::event::VirtualKeyCode;
use ggez::{Context, ContextBuilder, GameResult};

fn tile_rect(coords: Point2<i32>) -> Rect {
	Rect::new(
		coords.x as f32 * Tile::W,
		coords.y as f32 * Tile::H,
		Tile::W,
		Tile::H,
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
	Soap,
	Raygun,
	Mirror,
	MirrorSlopeUp,
	MirrorSlopeDown,
	Sapling,
	Tree,
	Axe,
	WallWithHoles,
	Cheese,
	Bunny,
}

impl Sprite {
	fn rect_in_spritesheet(self) -> Rect {
		let (x, y) = match self {
			Sprite::Player => (0, 0),
			Sprite::Grass => (1, 1),
			Sprite::Rock => (3, 0),
			Sprite::Wall => (2, 0),
			Sprite::Rope => (4, 0),
			Sprite::Soap => (5, 0),
			Sprite::Raygun => (2, 2),
			Sprite::Mirror => (3, 2),
			Sprite::MirrorSlopeUp => (4, 2),
			Sprite::MirrorSlopeDown => (5, 2),
			Sprite::Sapling => (3, 1),
			Sprite::Tree => (2, 1),
			Sprite::Axe => (4, 1),
			Sprite::WallWithHoles => (2, 3),
			Sprite::Cheese => (0, 1),
			Sprite::Bunny => (0, 3),
		};
		Rect::new(
			x as f32 * 8.0 / 128.0,
			y as f32 * 8.0 / 128.0,
			8.0 / 128.0,
			8.0 / 128.0,
		)
	}
}

fn draw_sprite(
	sprite: Sprite,
	dst: Rect,
	z: i32,
	color: Color,
	canvas: &mut Canvas,
	spritesheet: &Image,
) {
	let mut dst = dst;
	dst.w /= 8.0;
	dst.h /= 8.0; // Why is this needed ?
	canvas.draw(
		spritesheet,
		DrawParam::default()
			.dest_rect(dst)
			.src(sprite.rect_in_spritesheet())
			.z(z)
			.color(color),
	);
}

#[derive(Clone)]
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

#[derive(Clone, PartialEq, Eq)]
enum RaygunKind {
	/// Swap the shootee with the shooter.
	SwapWithShooter,
	/// Spawns a copy of the shootee on the tile the ray is coming from if possible.
	DuplicateShootee,
	/// Turns the shootee into the specified object.
	TurnInto(Box<ObjKind>),
	/// Turns the shootee *A* into a gun that turns its shootees into *A*.
	TurnIntoTurnInto,
}

impl RaygunKind {
	fn color(&self) -> Color {
		match self {
			RaygunKind::SwapWithShooter => Color::YELLOW,
			RaygunKind::DuplicateShootee => Color::CYAN,
			RaygunKind::TurnInto(_) => Color::WHITE,
			RaygunKind::TurnIntoTurnInto => Color::new(1.0, 0.6, 0.7, 1.0),
		}
	}
}

#[derive(Clone, PartialEq, Eq)]
enum ObjKind {
	/// Moved by arrow keys, can shoot guns. There can be multiple players.
	Player,
	/// Can be pushed (like most objects actually).
	Rock,
	/// Can *not* be pushed.
	Wall,
	/// Is pulled by anything that moves away, and pulls what is behind itself.
	Rope,
	/// Swaps places with what pushes it (or what follows it, etc.) instead of being pushed.
	Soap,
	/// Players can use these to shoot rays or various effects.
	Raygun(RaygunKind),
	/// Rays bounce back.
	Mirror,
	/// Rays bounce in an intuitive way on a `/` shaped mirror.
	MirrorSlopeUp,
	/// Rays bounce in an intuitive way on a `\` shaped mirror.
	MirrorSlopeDown,
	/// Can not be pushed, can be cut with an axe.
	Tree,
	/// Cuts down trees when pushed into them.
	Axe,
	/// Like a wall but lets rays through.
	WallWithHoles,
	/// Cheese.
	Cheese,
	/// Moves away from the player if it has line of sight. It is shy. Bnuuy.
	Bunny,
}

impl ObjKind {
	fn sprite_and_color(&self) -> (Sprite, Color) {
		let sprite = match self {
			ObjKind::Player => Sprite::Player,
			ObjKind::Rock => Sprite::Rock,
			ObjKind::Wall => Sprite::Wall,
			ObjKind::Rope => Sprite::Rope,
			ObjKind::Soap => Sprite::Soap,
			ObjKind::Raygun(_) => Sprite::Raygun,
			ObjKind::Mirror => Sprite::Mirror,
			ObjKind::MirrorSlopeUp => Sprite::MirrorSlopeUp,
			ObjKind::MirrorSlopeDown => Sprite::MirrorSlopeDown,
			ObjKind::Tree => Sprite::Tree,
			ObjKind::Axe => Sprite::Axe,
			ObjKind::WallWithHoles => Sprite::WallWithHoles,
			ObjKind::Cheese => Sprite::Cheese,
			ObjKind::Bunny => Sprite::Bunny,
		};
		let color = match self {
			ObjKind::Raygun(raygun_kind) => raygun_kind.color(),
			_ => Color::WHITE,
		};
		(sprite, color)
	}
}

#[derive(Clone)]
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
		!matches!(
			self.kind,
			ObjKind::Wall | ObjKind::Tree | ObjKind::WallWithHoles
		)
	}
}

#[derive(Clone)]
enum Ground {
	Grass,
	/// A sapling can be stepped on, then it will grow a tree when it can.
	Sapling {
		stepped_on: bool,
	},
}

#[derive(Clone)]
struct Tile {
	obj: Option<Obj>,
	ground: Ground,
}

impl Tile {
	const W: f32 = 80.0;
	const H: f32 = 80.0;

	fn new() -> Tile {
		Tile { obj: None, ground: Ground::Grass }
	}
}

#[derive(Clone)]
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

enum RayAction {
	SwapWith { with_who_coords: Point2<i32> },
	Duplicate,
	TurnInto { into_what: ObjKind },
	TurnIntoTurnInto,
}

struct Ray {
	coords: Point2<i32>,
	direction: IVec2,
	action: RayAction,
}

struct RaysAnimation {
	time_start: Instant,
	duration: Duration,
}

struct Level {
	grid: Grid,
	name: String,
	error_messages: Vec<String>,
}

impl Level {
	fn _test() -> Level {
		let mut grid = Grid::new();
		grid.get_mut(Point2::from([3, 5])).unwrap().obj = Some(Obj::from_kind(ObjKind::Player));
		//grid.get_mut(Point2::from([2, 5])).unwrap().obj = Some(Obj::from_kind(ObjKind::Player));
		grid.get_mut(Point2::from([5, 4])).unwrap().obj = Some(Obj::from_kind(ObjKind::Rock));
		grid.get_mut(Point2::from([5, 5])).unwrap().obj = Some(Obj::from_kind(ObjKind::Rock));
		grid.get_mut(Point2::from([5, 6])).unwrap().obj = Some(Obj::from_kind(ObjKind::Rope));
		grid.get_mut(Point2::from([5, 7])).unwrap().obj = Some(Obj::from_kind(ObjKind::Rope));
		grid.get_mut(Point2::from([2, 6])).unwrap().obj = Some(Obj::from_kind(ObjKind::Soap));
		grid.get_mut(Point2::from([3, 8])).unwrap().obj =
			Some(Obj::from_kind(ObjKind::Raygun(RaygunKind::SwapWithShooter)));
		grid.get_mut(Point2::from([4, 9])).unwrap().obj = Some(Obj::from_kind(ObjKind::Raygun(
			RaygunKind::DuplicateShootee,
		)));
		grid.get_mut(Point2::from([2, 9])).unwrap().obj = Some(Obj::from_kind(ObjKind::Raygun(
			RaygunKind::TurnInto(Box::new(ObjKind::Rock)),
		)));
		grid.get_mut(Point2::from([10, 2])).unwrap().obj = Some(Obj::from_kind(ObjKind::Raygun(
			RaygunKind::TurnIntoTurnInto,
		)));
		grid.get_mut(Point2::from([2, 2])).unwrap().obj = Some(Obj::from_kind(ObjKind::Wall));
		grid.get_mut(Point2::from([8, 8])).unwrap().obj = Some(Obj::from_kind(ObjKind::Mirror));
		grid.get_mut(Point2::from([8, 9])).unwrap().obj =
			Some(Obj::from_kind(ObjKind::MirrorSlopeDown));
		grid.get_mut(Point2::from([4, 11])).unwrap().obj =
			Some(Obj::from_kind(ObjKind::MirrorSlopeDown));
		grid.get_mut(Point2::from([8, 11])).unwrap().obj =
			Some(Obj::from_kind(ObjKind::MirrorSlopeUp));
		grid.get_mut(Point2::from([10, 10])).unwrap().obj = Some(Obj::from_kind(ObjKind::Tree));
		grid.get_mut(Point2::from([4, 4])).unwrap().ground = Ground::Sapling { stepped_on: false };
		grid.get_mut(Point2::from([9, 10])).unwrap().obj = Some(Obj::from_kind(ObjKind::Axe));
		grid.get_mut(Point2::from([4, 2])).unwrap().obj =
			Some(Obj::from_kind(ObjKind::WallWithHoles));
		grid.get_mut(Point2::from([10, 4])).unwrap().obj = Some(Obj::from_kind(ObjKind::Cheese));
		grid.get_mut(Point2::from([10, 6])).unwrap().obj = Some(Obj::from_kind(ObjKind::Bunny));

		Level { grid, name: "test".to_string(), error_messages: vec![] }
	}

	fn load_from_text(text: &str) -> Level {
		let mut grid = Grid::new();
		let mut chars_to_coords: HashMap<char, Vec<Point2<i32>>> = HashMap::new();
		let mut name = "name".to_string();
		let mut error_messages = vec![];
		let mut lines = text.lines().enumerate();
		while let Some((line_index, line)) = lines.next() {
			let line_number = line_index + 1;
			let words: Vec<_> = line.split_ascii_whitespace().collect();
			if words.is_empty() {
				continue;
			}
			match words[0] {
				"name" => {
					if words.len() >= 2 {
						name = words[1..].join(" ").to_string();
					} else {
						error_messages.push(format!(
							"syntax error: missing name argument at line {line_number}"
						));
					}
				},
				"grid" => {
					for grid_row_index in 0..Grid::H {
						let grid_row_number = grid_row_index + 1;
						let (_line_index, line) = if let Some(line) = lines.next() {
							line
						} else {
							error_messages.push(format!(
								"syntax error: missing {grid_row_number}-th grid row at end of file"
							));
							break;
						};
						for (x, character) in line
							.chars()
							.enumerate()
							.filter_map(|(i, c)| if i % 2 == 0 { Some(c) } else { None })
							.enumerate()
						{
							let coords = Point2::from([x as i32, grid_row_index]);
							let entry = chars_to_coords.entry(character);
							entry.or_default().push(coords);
						}
					}
				},
				"obj" => {
					let character = if let Some(word) = words.get(1) {
						if *word == "space" {
							' '
						} else if word.len() == 1 {
							word.chars().next().unwrap()
						} else {
							error_messages.push(format!(
								"syntax error: should be a single character after \"obj\" at line {line_number}"
							));
							continue;
						}
					} else {
						error_messages.push(format!(
							"syntax error: missing character after \"obj\" at line {line_number}"
						));
						continue;
					};
					let obj_descr = if let Some(word) = words.get(2) {
						word
					} else {
						error_messages.push(format!(
							"syntax error: missing object description after \"obj\" at line {line_number}"
						));
						continue;
					};
					fn parse_obj_descr(descr: &str, line_number: usize) -> Result<Option<Obj>, String> {
						Ok(match descr {
							"none" => None,
							"player" => Some(Obj::from_kind(ObjKind::Player)),
							"rock" => Some(Obj::from_kind(ObjKind::Rock)),
							"wall" => Some(Obj::from_kind(ObjKind::Wall)),
							"rope" => Some(Obj::from_kind(ObjKind::Rope)),
							"soap" => Some(Obj::from_kind(ObjKind::Soap)),
							"mirror" => Some(Obj::from_kind(ObjKind::Mirror)),
							"mirror_slope_up" => Some(Obj::from_kind(ObjKind::MirrorSlopeUp)),
							"mirror_slope_down" => Some(Obj::from_kind(ObjKind::MirrorSlopeDown)),
							"tree" => Some(Obj::from_kind(ObjKind::Tree)),
							"axe" => Some(Obj::from_kind(ObjKind::Axe)),
							"wall_with_holes" => Some(Obj::from_kind(ObjKind::WallWithHoles)),
							"cheese" => Some(Obj::from_kind(ObjKind::Cheese)),
							"bunny" => Some(Obj::from_kind(ObjKind::Bunny)),
							raygun if raygun.starts_with("raygun") => {
								let raygun_kind = match raygun.split(':').nth(1) {
									Some("swap") => RaygunKind::SwapWithShooter,
									Some("duplicate") => RaygunKind::DuplicateShootee,
									Some("turn_into_turn_into") => RaygunKind::TurnIntoTurnInto,
									Some("turn_into") => {
										let index = if let Some((index, _)) = raygun.match_indices(':').nth(1)
										{
											index
										} else {
											return Err(format!(
												"syntax error: missing object after \"turn_into\" at line {line_number}"
											));
										};
										let turn_into_what =
											parse_obj_descr(&raygun[(index + 1)..], line_number)?;
										let turn_into_what_kind = if let Some(obj) = turn_into_what {
											obj.kind
										} else {
											return Err(format!(
												"syntax error: \"turn_into\" none is not allowed at line {line_number}"
											));
										};
										RaygunKind::TurnInto(Box::new(turn_into_what_kind))
									},
									Some(unknown_kind) => {
										return Err(format!(
											"syntax error: unknown raygun kind \"{unknown_kind}\" at line {line_number}"
										));
									},
									None => {
										return Err(format!(
											"syntax error: missing raygun model at line {line_number}"
										));
									},
								};
								Some(Obj::from_kind(ObjKind::Raygun(raygun_kind)))
							},
							unknown_obj => {
								return Err(format!(
									"syntax error: unknown object \"{unknown_obj}\" at line {line_number}"
								));
							},
						})
					}
					let obj = match parse_obj_descr(obj_descr, line_number) {
						Ok(obj) => obj,
						Err(error) => {
							error_messages.push(error);
							continue;
						},
					};
					if let Some(coords_list) = chars_to_coords.get(&character) {
						for coords in coords_list {
							grid.get_mut(*coords).unwrap().obj = obj.clone();
						}
					}
				},
				unknown_word => error_messages.push(format!(
					"syntax error: unknown \"{unknown_word}\" at line {line_number}"
				)),
			}
		}
		Level { grid, name, error_messages }
	}
}

struct Game {
	level: Level,
	grid: Grid,
	rays: Vec<Ray>,
	rays_animation: Option<RaysAnimation>,
	spritesheet: Image,
	cheese_count: u32,
	step_count: u32,
}

impl Game {
	pub fn new(ctx: &mut Context) -> GameResult<Game> {
		let level = Level::_test();
		//let level = Level::load_from_text(&std::fs::read_to_string("levels/test04.puzhlvl").unwrap());
		let grid = level.grid.clone();
		Ok(Game {
			level,
			grid,
			rays: vec![],
			rays_animation: None,
			spritesheet: Image::from_bytes(ctx, include_bytes!("../assets/spritesheet.png"))?,
			cheese_count: 0,
			step_count: 0,
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

	fn handle_sapling(&mut self, can_grow: bool) {
		for tile in self.grid.tiles.iter_mut() {
			if let Ground::Sapling { stepped_on } = tile.ground {
				if stepped_on && tile.obj.is_none() && can_grow {
					tile.ground = Ground::Grass;
					tile.obj = Some(Obj::from_kind(ObjKind::Tree));
				} else if (!stepped_on) && tile.obj.is_some() {
					tile.ground = Ground::Sapling { stepped_on: true };
				}
			}
		}
	}

	fn line_of_sights_to(&self, coords: Point2<i32>, to_what: ObjKind) -> Vec<IVec2> {
		[(1, 0), (0, 1), (-1, 0), (0, -1)]
			.into_iter()
			.map(|(dx, dy)| IVec2::from([dx, dy]))
			.filter(|&direction| {
				let mut coords = IVec2::from(coords);
				loop {
					coords += direction;
					if let Some(tile) = self.grid.get(coords.into()) {
						if let Some(obj) = &tile.obj {
							break obj.kind == to_what;
						}
					} else {
						break false;
					}
				}
			})
			.collect()
	}

	fn handle_bunnies(&mut self) {
		for grid_y in 0..Grid::H {
			for grid_x in 0..Grid::W {
				let coords = Point2::from([grid_x, grid_y]);
				if let Some(obj) = &self.grid.get(coords).unwrap().obj {
					if obj.kind == ObjKind::Bunny && !obj.processed {
						let mut scarred_dirs = self.line_of_sights_to(coords, ObjKind::Player);
						scarred_dirs.retain(|&dir| {
							let tile = self.grid.get((IVec2::from(coords) - dir).into());
							tile.is_some_and(|tile| {
								tile.obj.is_none() || tile.obj.as_ref().is_some_and(|obj| obj.can_move())
							})
						});
						let scarred_dir: IVec2 = scarred_dirs.into_iter().sum();
						if scarred_dir.x.abs() + scarred_dir.y.abs() == 1 {
							self
								.grid
								.get_mut(coords)
								.unwrap()
								.obj
								.as_mut()
								.unwrap()
								.processed = true;
							self.obj_move(coords, -scarred_dir, false);
						}
					}
				}
			}
		}
	}

	fn obj_move(&mut self, coords: Point2<i32>, direction: IVec2, pushed: bool) {
		let coords_dst = IVec2::from(coords) + direction;
		let mut shall_move = false;
		let mut failed_to_move = false;
		let mut soap_getting_back = None;
		if let Some(tile) = self.grid.get(coords) {
			if let Some(obj) = &tile.obj {
				if obj.can_move() {
					if let Some(tile_dst) = self.grid.get(coords_dst.into()) {
						if let Some(obj_dst) = &tile_dst.obj {
							if matches!(obj_dst.kind, ObjKind::Soap) {
								soap_getting_back =
									self.grid.get_mut(coords_dst.into()).unwrap().obj.take();
							} else if matches!(obj.kind, ObjKind::Axe)
								&& matches!(obj_dst.kind, ObjKind::Tree)
							{
								self.grid.get_mut(coords_dst.into()).unwrap().obj = None;
							} else if matches!(obj.kind, ObjKind::Player)
								&& matches!(obj_dst.kind, ObjKind::Cheese)
							{
								self.grid.get_mut(coords_dst.into()).unwrap().obj = None;
								self.cheese_count += 1;
							} else {
								self.obj_move(coords_dst.into(), direction, true);
							}
						}
					}
					if let Some(tile_dst) = self.grid.get(coords_dst.into()) {
						if let Some(obj_dst) = &tile_dst.obj {
							if matches!(obj_dst.kind, ObjKind::Soap) {
								soap_getting_back =
									self.grid.get_mut(coords_dst.into()).unwrap().obj.take();
							}
						}
					}
					if let Some(tile_dst) = self.grid.get(coords_dst.into()) {
						if tile_dst.obj.is_none() {
							shall_move = true;
						} else {
							failed_to_move = true;
						}
					} else {
						failed_to_move = true;
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

			if let Some(mut soap) = soap_getting_back.take() {
				if matches!(soap.animation, Animation::None) {
					soap.animation = Animation::CommingFrom {
						src: coords_dst.into(),
						time_start: Instant::now(),
						duration: Duration::from_secs_f32(0.05),
					};
					soap.moved = true;
				}
				self.grid.get_mut(coords).unwrap().obj = Some(soap);
			}

			self.handle_sapling(false);
		} else if failed_to_move {
			if let Some(obj) = self.grid.get_mut(coords).unwrap().obj.as_mut() {
				obj.animation = Animation::FailingToMoveTo {
					dst: coords_dst.into(),
					time_start: Instant::now(),
					duration: Duration::from_secs_f32(0.05),
				};
			}
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

		self.step_count += 1;
		self.handle_sapling(true);
		self.handle_bunnies();
	}

	fn player_shoot(&mut self) {
		self.clear_processed_flags();
		self.clear_moved_flags();
		self.clear_animations();

		for grid_y in 0..Grid::H {
			for grid_x in 0..Grid::W {
				let coords = Point2::from([grid_x, grid_y]);
				if let Some(obj) = &self.grid.get(coords).unwrap().obj {
					if matches!(obj.kind, ObjKind::Player) && !obj.processed {
						self
							.grid
							.get_mut(coords)
							.unwrap()
							.obj
							.as_mut()
							.unwrap()
							.processed = true;
						for move_to_neighboor in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
							let (dx, dy) = move_to_neighboor;
							let player_to_neighboor = IVec2::from([dx, dy]);
							let neighboor_coords = IVec2::from(coords) + player_to_neighboor;
							if let Some(neighboor_obj) = &self
								.grid
								.get(neighboor_coords.into())
								.and_then(|tile| tile.obj.as_ref())
							{
								if let ObjKind::Raygun(kind) = neighboor_obj.kind.clone() {
									self.rays.push(Ray {
										coords: neighboor_coords.into(),
										direction: player_to_neighboor,
										action: match kind {
											RaygunKind::SwapWithShooter => {
												RayAction::SwapWith { with_who_coords: coords }
											},
											RaygunKind::DuplicateShootee => RayAction::Duplicate,
											RaygunKind::TurnInto(into_what) => {
												RayAction::TurnInto { into_what: *into_what }
											},
											RaygunKind::TurnIntoTurnInto => RayAction::TurnIntoTurnInto,
										},
									})
								}
							}
						}
					}
				}
			}
		}
	}
}

impl EventHandler for Game {
	fn update(&mut self, _ctx: &mut Context) -> GameResult {
		if !self.rays.is_empty() {
			if self.rays_animation.is_none() {
				self.rays_animation = Some(RaysAnimation {
					time_start: Instant::now(),
					duration: Duration::from_secs_f32(0.02),
				})
			}

			if let Some(RaysAnimation { time_start, duration }) = self.rays_animation {
				let progress = time_start.elapsed().as_secs_f32() / duration.as_secs_f32();
				if progress >= 1.0 {
					self.rays_animation = None;
					let mut rays_indices_to_remove = vec![];
					for (ray_index, ray) in self.rays.iter_mut().enumerate() {
						let dst_coords = IVec2::from(ray.coords) + ray.direction;
						if let Some(dst_tile) = self.grid.get(dst_coords.into()) {
							if dst_tile
								.obj
								.as_ref()
								.is_some_and(|obj| matches!(obj.kind, ObjKind::WallWithHoles))
							{
								ray.coords = dst_coords.into();
							} else if dst_tile
								.obj
								.as_ref()
								.is_some_and(|obj| matches!(obj.kind, ObjKind::Mirror))
							{
								ray.coords = dst_coords.into();
								ray.direction = -ray.direction;
							} else if dst_tile
								.obj
								.as_ref()
								.is_some_and(|obj| matches!(obj.kind, ObjKind::MirrorSlopeUp))
							{
								ray.coords = dst_coords.into();
								let dir = ray.direction;
								ray.direction.y = -dir.x;
								ray.direction.x = -dir.y;
							} else if dst_tile
								.obj
								.as_ref()
								.is_some_and(|obj| matches!(obj.kind, ObjKind::MirrorSlopeDown))
							{
								ray.coords = dst_coords.into();
								let dir = ray.direction;
								ray.direction.y = dir.x;
								ray.direction.x = dir.y;
							} else if dst_tile.obj.is_some() {
								match ray.action {
									RayAction::SwapWith { with_who_coords } => {
										rays_indices_to_remove.push(ray_index);
										let shootee =
											self.grid.get_mut(dst_coords.into()).unwrap().obj.take();
										let shooter = self.grid.get_mut(with_who_coords).unwrap().obj.take();
										self.grid.get_mut(dst_coords.into()).unwrap().obj = shooter;
										self.grid.get_mut(with_who_coords).unwrap().obj = shootee;
									},
									RayAction::Duplicate => {
										rays_indices_to_remove.push(ray_index);
										let shootee_kind = self
											.grid
											.get(dst_coords.into())
											.unwrap()
											.obj
											.as_ref()
											.unwrap()
											.kind
											.clone();
										let obj_to_be_duplicated_to =
											&mut self.grid.get_mut(ray.coords).unwrap().obj;
										if obj_to_be_duplicated_to.is_none() {
											*obj_to_be_duplicated_to = Some(Obj::from_kind(shootee_kind));
										}
									},
									RayAction::TurnInto { ref into_what } => {
										rays_indices_to_remove.push(ray_index);
										self.grid.get_mut(dst_coords.into()).unwrap().obj =
											Some(Obj::from_kind(into_what.clone()));
									},
									RayAction::TurnIntoTurnInto => {
										rays_indices_to_remove.push(ray_index);
										let shootee = self
											.grid
											.get_mut(dst_coords.into())
											.unwrap()
											.obj
											.take()
											.unwrap();
										self.grid.get_mut(dst_coords.into()).unwrap().obj =
											Some(Obj::from_kind(ObjKind::Raygun(RaygunKind::TurnInto(
												Box::new(shootee.kind),
											))));
									},
								}
							} else {
								ray.coords = dst_coords.into();
							}
						} else {
							rays_indices_to_remove.push(ray_index);
						}
					}
					rays_indices_to_remove.sort();
					for index_to_remove in rays_indices_to_remove.into_iter().rev() {
						self.rays.remove(index_to_remove);
					}
					self.handle_sapling(true);
				}
			}
		}

		Ok(())
	}

	fn key_down_event(&mut self, ctx: &mut Context, input: KeyInput, _repeated: bool) -> GameResult {
		let can_play = self.rays.is_empty();
		match input.keycode {
			Some(VirtualKeyCode::Escape) => ctx.request_quit(),
			Some(VirtualKeyCode::R) => {
				self.rays = vec![];
				self.grid = self.level.grid.clone();
			},
			Some(VirtualKeyCode::Up) if can_play => self.player_move(IVec2::from([0, -1])),
			Some(VirtualKeyCode::Down) if can_play => self.player_move(IVec2::from([0, 1])),
			Some(VirtualKeyCode::Left) if can_play => self.player_move(IVec2::from([-1, 0])),
			Some(VirtualKeyCode::Right) if can_play => self.player_move(IVec2::from([1, 0])),
			Some(VirtualKeyCode::Space) | Some(VirtualKeyCode::Return) if can_play => {
				self.player_shoot()
			},
			_ => {},
		}

		Ok(())
	}

	fn draw(&mut self, ctx: &mut Context) -> GameResult {
		let mut canvas = Canvas::from_frame(ctx, Color::BLACK);
		canvas.set_sampler(graphics::Sampler::nearest_clamp());

		for ray in self.rays.iter() {
			let center = if let Some(RaysAnimation { time_start, duration }) = self.rays_animation {
				let dst = IVec2::from(ray.coords) + ray.direction;
				let center_src = tile_rect(ray.coords).center();
				let center_dst = tile_rect(dst.into()).center();
				let progress = time_start.elapsed().as_secs_f32() / duration.as_secs_f32();
				let progress = progress.clamp(0.0, 1.0);
				let window_x = lerp(progress, center_src.x, center_dst.x);
				let window_y = lerp(progress, center_src.y, center_dst.y);
				Point2::from([window_x, window_y])
			} else {
				tile_rect(ray.coords).center()
			};
			let a = Vec2::from(center) + ray.direction.as_vec2() * 0.5 * Vec2::new(Tile::W, Tile::H);
			let b = Vec2::from(center) - ray.direction.as_vec2() * 0.5 * Vec2::new(Tile::W, Tile::H);
			let raygun_kind = match ray.action {
				RayAction::SwapWith { .. } => RaygunKind::SwapWithShooter,
				RayAction::Duplicate => RaygunKind::DuplicateShootee,
				RayAction::TurnInto { ref into_what } => {
					RaygunKind::TurnInto(Box::new(into_what.clone()))
				},
				RayAction::TurnIntoTurnInto => RaygunKind::TurnIntoTurnInto,
			};
			let color = raygun_kind.color();
			canvas.draw(
				&graphics::Mesh::new_line(ctx, &[a, b], 10.0, color)?,
				DrawParam::default().z(4),
			);
		}

		for grid_y in 0..Grid::H {
			for grid_x in 0..Grid::W {
				let coords = Point2::from([grid_x, grid_y]);

				draw_sprite(
					Sprite::Grass,
					tile_rect(coords),
					1,
					Color::WHITE,
					&mut canvas,
					&self.spritesheet,
				);
				if matches!(
					self
						.grid
						.get(Point2::from([grid_x, grid_y]))
						.unwrap()
						.ground,
					Ground::Sapling { .. }
				) {
					draw_sprite(
						Sprite::Sapling,
						tile_rect(coords),
						2,
						Color::WHITE,
						&mut canvas,
						&self.spritesheet,
					);
				}

				if let Some(obj) = &self.grid.get(Point2::from([grid_x, grid_y])).unwrap().obj {
					let (sprite, color) = obj.kind.sprite_and_color();
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
					draw_sprite(sprite, rect, 3, color, &mut canvas, &self.spritesheet);

					// TurnInto rayguns display what they turn their targets into on them.
					// This is kinda recursive is they can turn targets into TurnInto guns etc.
					if let ObjKind::Raygun(RaygunKind::TurnInto(into_what)) = &obj.kind {
						let size = 4.0 * 8.0;
						let sub_rect = Rect::new(rect.right() - size, rect.bottom() - size, size, size);
						let (sprite, color) = into_what.sprite_and_color();
						draw_sprite(sprite, sub_rect, 4, color, &mut canvas, &self.spritesheet);
						if let ObjKind::Raygun(RaygunKind::TurnInto(into_what)) = &**into_what {
							let size = 2.0 * 8.0;
							let sub_rect =
								Rect::new(rect.right() - size, rect.bottom() - size, size, size);
							let (sprite, color) = into_what.sprite_and_color();
							draw_sprite(sprite, sub_rect, 5, color, &mut canvas, &self.spritesheet);
							if let ObjKind::Raygun(RaygunKind::TurnInto(into_what)) = &**into_what {
								let size = 1.0 * 8.0;
								let sub_rect =
									Rect::new(rect.right() - size, rect.bottom() - size, size, size);
								let (sprite, color) = into_what.sprite_and_color();
								draw_sprite(sprite, sub_rect, 6, color, &mut canvas, &self.spritesheet);
								if let ObjKind::Raygun(RaygunKind::TurnInto(into_what)) = &**into_what {
									let size = 0.5 * 8.0;
									let sub_rect =
										Rect::new(rect.right() - size, rect.bottom() - size, size, size);
									let (sprite, color) = into_what.sprite_and_color();
									draw_sprite(sprite, sub_rect, 7, color, &mut canvas, &self.spritesheet);
								}
							}
						}
					}
				}
			}
		}

		let mut text_y = 0.0;
		{
			let mut text = graphics::Text::new(&self.level.name);
			let scale = 30.0;
			text.set_scale(scale);
			canvas.draw(
				&text,
				DrawParam::default()
					.z(8)
					.color(Color::BLACK)
					.offset(Vec2::from([0.0, text_y])),
			);
			text_y -= scale;
		}

		if self.cheese_count >= 1 {
			let cheese_text = format!(
				"{} cheese{}",
				self.cheese_count,
				if self.cheese_count >= 2 { "s" } else { "" }
			);
			let mut text = graphics::Text::new(cheese_text);
			let scale = 30.0;
			text.set_scale(scale);
			canvas.draw(
				&text,
				DrawParam::default()
					.z(8)
					.color(Color::BLACK)
					.offset(Vec2::from([0.0, text_y])),
			);
			text_y -= scale;
		}

		{
			let mut text = graphics::Text::new(&format!(" {} steps", self.step_count));
			let scale = 20.0;
			text.set_scale(scale);
			canvas.draw(
				&text,
				DrawParam::default()
					.z(8)
					.color(Color::BLACK)
					.offset(Vec2::from([0.0, text_y])),
			);
			text_y -= scale;
		}

		for error_message in self.level.error_messages.iter() {
			let mut text = graphics::Text::new(error_message);
			let scale = 20.0;
			text.set_scale(scale);
			canvas.draw(
				&text,
				DrawParam::default()
					.z(8)
					.color(Color::new(0.6, 0.0, 0.0, 1.0))
					.offset(Vec2::from([0.0, text_y])),
			);
			text_y -= scale;
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
