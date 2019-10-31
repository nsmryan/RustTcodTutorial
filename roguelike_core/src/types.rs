use std::convert::Into;

use tcod::map::{Map as FovMap};

use num::clamp;

use crate::map::*;


pub type ObjectId = usize;


#[derive(Eq, PartialEq, Copy, Clone)]
pub enum GameState {
    Playing,
    Win,
    Lose,
}

pub struct GameData {
    pub map: Map,
    pub objects: Vec<Object>,
    pub fov: FovMap,
}

impl GameData {
    pub fn new(map: Map, objects: Vec<Object>, fov: FovMap) -> GameData {
        GameData {
            map,
            objects,
            fov,
        }
    }
}

pub struct GameSettings {
    pub previous_player_position: (i32, i32),
    pub turn_count: usize,
    pub god_mode: bool,
}

impl GameSettings {
    pub fn new(previous_player_position: (i32, i32),
               turn_count: usize,
               god_mode: bool) -> GameSettings {
        GameSettings {
            previous_player_position,
            turn_count,
            god_mode,
        }
    }
}


// TODO pressed state should be broken out, not in a tuple
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct MouseState {
    pub pos: (i32, i32),
    pub pressed: (bool, bool, bool),
    pub wheel: f32,
}


#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reach {
    Single(usize),
    Diag(usize),
    Horiz(usize),
}

impl Reach {
    pub fn move_with_reach(&self, move_action: &MoveAction) -> Option<Position> {
        match self {
            Reach::Single(dist) => {
                let dist = (*dist) as i32;
                let neg_dist = dist * -1;
                match move_action {
                    MoveAction::Left => Some(Position::from_pair((neg_dist, 0))),
                    MoveAction::Right => Some(Position::from_pair((dist, 0))),
                    MoveAction::Up => Some(Position::from_pair((0, neg_dist))),
                    MoveAction::Down => Some(Position::from_pair((0, dist))),
                    MoveAction::DownLeft => Some(Position::from_pair((neg_dist, dist))),
                    MoveAction::DownRight => Some(Position::from_pair((dist, dist))),
                    MoveAction::UpLeft => Some(Position::from_pair((neg_dist, neg_dist))),
                    MoveAction::UpRight => Some(Position::from_pair((dist, neg_dist))),
                    MoveAction::Center => Some(Position::from_pair((0, 0))),
                }
            }

            Reach::Diag(dist) => {
                let dist = (*dist) as i32;
                let neg_dist = dist * -1;
                match move_action {
                    MoveAction::Left => None,
                    MoveAction::Right => None,
                    MoveAction::Up => None,
                    MoveAction::Down => None,
                    MoveAction::DownLeft => Some(Position::from_pair((neg_dist, dist))),
                    MoveAction::DownRight => Some(Position::from_pair((dist, dist))),
                    MoveAction::UpLeft => Some(Position::from_pair((neg_dist, neg_dist))),
                    MoveAction::UpRight => Some(Position::from_pair((dist, neg_dist))),
                    MoveAction::Center => Some(Position::from_pair((0, 0))),
                }
            }

            Reach::Horiz(dist) => {
                let dist = (*dist) as i32;
                let neg_dist = dist * -1;
                match move_action {
                    MoveAction::Left => Some(Position::from_pair((neg_dist, 0))),
                    MoveAction::Right => Some(Position::from_pair((dist, 0))),
                    MoveAction::Up => Some(Position::from_pair((0, neg_dist))),
                    MoveAction::Down => Some(Position::from_pair((0, dist))),
                    MoveAction::DownLeft => None,
                    MoveAction::DownRight => None,
                    MoveAction::UpLeft => None,
                    MoveAction::UpRight => None,
                    MoveAction::Center => None,
                }
            }
        }
    }

    pub fn offsets(&self) -> Vec<Position> {
        match self {
            Reach::Single(dist) => {
                let dist = (*dist) as i32;
                let offsets =
                    vec!( (0, dist),      (-dist, dist), (-dist,  0),
                          (-dist, -dist), (0,  -dist),   (dist, -dist),
                          (dist,  0), (dist, dist));
                offsets.iter().map(|pair| Position::from_pair(*pair)).collect()
            },

            Reach::Horiz(dist) => {
                let dist = (*dist) as i32;
                let mut offsets = vec!();
                for dist in 1..dist {
                    offsets.push((dist, 0));
                    offsets.push((0, dist));
                    offsets.push((-1 * dist, 0));
                    offsets.push((0, -1 * dist));
                }
                offsets.iter().map(|pair| Position::from_pair(*pair)).collect()
            },


            Reach::Diag(dist) => {
                let mut offsets = vec!();
                let dist = (*dist) as i32;
                for dist in 1..dist {
                    offsets.push((dist, dist));
                    offsets.push((-1 * dist, dist));
                    offsets.push((dist, -1 * dist));
                    offsets.push((-1 * dist, -1 * dist));
                }
                offsets.iter().map(|pair| Position::from_pair(*pair)).collect()
            },
        }
    }
}


#[derive(Clone, Debug, PartialEq)]
pub enum Animation {
    Idle(),
}


#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub x: i32,
    pub y: i32,
    pub char: char,
    pub name: String,
    pub blocks: bool,
    pub alive: bool,
    pub fighter: Option<Fighter>,
    pub ai: Option<Ai>,
    pub behavior: Option<Behavior>,
    pub item: Option<Item>,
    pub momentum: Option<Momentum>,
    pub movement: Option<Reach>,
    pub attack: Option<Reach>,
    pub animation: Option<Animation>,
}

impl Object {
    pub fn new(x: i32, y: i32, chr: char, name: &str, blocks: bool) -> Self {
        Object {
            x: x,
            y: y,
            char: chr,
            name: name.into(),
            blocks: blocks,
            alive: false,
            fighter: None,
            ai: None,
            behavior: None,
            item: None,        
            momentum: None,
            movement: None,
            attack: None,
            animation: None,
        }
    }

    pub fn pos(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn distance_to(&self, other: &Object) -> f32 {
        return self.distance(&Position::new(other.x, other.y));
    }

    pub fn distance(&self, other: &Position) -> f32 {
        let dx = other.0 - self.x;
        let dy = other.1 - self.y;
        return ((dx.pow(2) + dy.pow(2)) as f32).sqrt();
    }

    pub fn take_damage(&mut self, damage: i32) {
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
        }

        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.alive = false;
                fighter.on_death.callback(self);
            }
        }
    }

    pub fn attack(&mut self, target: &mut Object) {
        let damage = self.fighter.map_or(0, |f| f.power) - target.fighter.map_or(0, |f| f.defense);

        if damage > 0 {
            //messages.message(format!("{} attacks {} for {} hit points.", self.name, target.name, damage), WHITE);
            target.take_damage(damage);
        } else {
            //messages.message(format!("{} attacks {} but it has no effect!", self.name, target.name), WHITE);
        }
    }

    pub fn heal(&mut self, amount: i32) {
        if let Some(ref mut fighter) = self.fighter {
            fighter.hp += amount;
            if fighter.hp > fighter.max_hp {
                fighter.hp = fighter.max_hp;
            }
        }
    }

    pub fn make_stone(x: i32, y: i32) -> Object {
        Object::new(x, y, 'o', "stone", false)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PatrolDir {
    Forward,
    Reverse,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AwarenessMap {
    pub weights: Vec<Vec<f32>>,
    pub alt_weights: Vec<Vec<f32>>,
    pub width: usize,
    pub height: usize,
}

impl AwarenessMap {
    pub fn new(width: usize, height: usize) -> AwarenessMap {
        AwarenessMap {
            weights: vec![vec![0.0; width]; height],
            alt_weights: vec![vec![0.0; width]; height],
            width: width,
            height: height,
        }
    }

    pub fn expected_position(&mut self, position: Position) {
        for y in 0..self.height {
            for x in 0..self.width {
                if (x as i32, y as i32) == position.pair() {
                    self.weights[y][x] = 1.0;
                } else {
                    self.weights[y][x] = 0.0;
                }
            }
        }
    }

    pub fn visible(&mut self, position: Position) {
        self.weights[position.1 as usize][position.0 as usize] = 0.0;
    }

    pub fn disperse(&mut self) {
        for y in 0..self.height {
            for x in 0..self.width {
                let potential_positions =
                    vec![(x + 1, y),     (x + 1, y + 1), (x + 1, y - 1),
                    (x,     y + 1), (x,     y - 1), (x - 1, y),
                    (x - 1, y + 1), (x - 1, y - 1)];
                let _potential_positions =
                    potential_positions.iter()
                    .filter(|(x, y)| *x < self.width && *y < self.height)
                    .filter(|(x, y)| self.weights[*y as usize][*x as usize] > 0.0);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Item {
    Stone,
    Goal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UseResult {
    UsedUp,
    Cancelled,
    Keep,
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PlayerAction {
    TookTurn,
    TookHalfTurn,
    DidntTakeTurn,
    Exit,
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MoveAction {
    Left,
    Right,
    Up,
    Down,
    DownLeft,
    DownRight,
    UpLeft,
    UpRight,
    Center,
}

impl MoveAction {
    pub fn into_move(self) -> (i32, i32) {
        match self {
            MoveAction::Left => (-1, 0),
            MoveAction::Right => (1, 0),
            MoveAction::Up => (0, -1),
            MoveAction::Down => (0, 1),
            MoveAction::DownLeft => (-1, 1),
            MoveAction::DownRight => (1, 1),
            MoveAction::UpLeft => (-1, -1),
            MoveAction::UpRight => (1, -1),
            MoveAction::Center => (0, 0),
        }
    }

    pub fn move_actions() -> Vec<MoveAction> {
        return vec!(MoveAction::Left,
                    MoveAction::Right,
                    MoveAction::Up,
                    MoveAction::Down,
                    MoveAction::DownLeft,
                    MoveAction::DownRight,
                    MoveAction::UpLeft,
                    MoveAction::UpRight,
                    MoveAction::Center);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputAction {
    Move(MoveAction),
    Pickup,
    Inventory,
    Exit,
    ExploreAll,
    RegenerateMap,
    ToggleOverlays,
    GodMode,
    FullScreen,
    None,
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ai {
    Basic,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Behavior {
    Idle,
    Investigating(Position),
    Attacking(ObjectId),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AiAction {
    Move((i32, i32)),
    Attack(ObjectId, (i32, i32)),
    StateChange(Behavior),
}


#[derive(Clone, Debug, PartialEq)]
pub struct AiTurn(Vec<AiAction>);

impl AiTurn {
    pub fn new() -> AiTurn {
        return AiTurn(Vec::new());
    }

    pub fn add(&mut self, action: AiAction) {
        self.0.push(action);
    }

    pub fn actions(self) -> Vec<AiAction> {
        return self.0;
    }
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fighter {
    pub max_hp: i32,
    pub hp: i32,
    pub defense: i32,
    pub power: i32,
    pub on_death: DeathCallback,
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DeathCallback {
    Player,
    Monster,
}

impl DeathCallback {
    fn callback(self, object: &mut Object) {
        use DeathCallback::*;

        let callback: fn(&mut Object) = match self {
            Player => player_death,
            Monster => monster_death,
        };

        callback(object);
    }
}

pub fn player_death(player: &mut Object) {
    player.char = '%';
}

pub fn monster_death(monster: &mut Object) {
    monster.char = '%';
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("remains of {}", monster.name);
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Momentum {
    pub mx: i32,
    pub my: i32,
    pub took_half_turn: bool,
    pub max: i32,
}

impl Default for Momentum {
    fn default() -> Momentum {
        Momentum {
            mx: 0,
            my: 0,
            took_half_turn: false,
            max: 2, // TODO make this configurable
        }
    }
}

impl Momentum {
    pub fn running(&mut self) -> bool {
        return self.magnitude() != 0;
    }

    pub fn magnitude(&self) -> i32 {
        if self.mx.abs() > self.my.abs() {
            return self.mx.abs();
        } else {
            return self.my.abs();
        }
    }

    pub fn diagonal(&self) -> bool {
        return self.mx.abs() != 0 && self.my.abs() != 0;
    }

    pub fn moved(&mut self, dx: i32, dy: i32) {
        // if the movement is in the opposite direction, and we have some momentum
        // currently, lose our momentum.

        if self.mx != 0 && dx.signum() != self.mx.signum() {
            self.mx = 0;
        } else {
            self.mx = clamp(self.mx + dx.signum(), -self.max, self.max);
        }

        if self.my != 0 && dy.signum() != self.my.signum() {
            self.my = 0;
        } else {
            self.my = clamp(self.my + dy.signum(), -self.max, self.max);
        }
    }

    pub fn set_momentum(&mut self, mx: i32, my: i32) {
        self.mx = mx;
        self.my = my;
    }

    pub fn clear(&mut self) {
        self.mx = 0;
        self.my = 0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Movement {
    Move(i32, i32),
    Attack(i32, i32, ObjectId),
    Collide(i32, i32),
    WallKick(i32, i32, i32, i32), // x, y, dir_x, dir_y
    JumpWall(i32, i32),
}

impl Movement {
    pub fn xy(&self) -> (i32, i32) {
        match self {
            Movement::Move(x, y) => (*x, *y),
            Movement::Attack(x, y, _) => (*x, *y),
            Movement::Collide(x, y) => (*x, *y),
            Movement::WallKick(x, y, _, _) => (*x, *y),
            Movement::JumpWall(x, y) => (*x, *y),
        }
    }
}


#[derive(Clone, Copy, Debug)]
pub struct Rect  {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x1: x, y1: y, x2: x + w, y2: y + h }
    }

    pub fn center(&self) -> (i32, i32) {
        let center_x = (self.x1 + self.x2) / 2;
        let center_y = (self.y1 + self.y2) / 2;
        (center_x, center_y)
    }

    pub fn intersects_with(&self, other: &Rect) -> bool {
        (self.x1 <= other.x2) &&
            (self.x2 >= other.x1) &&
            (self.y1 <= other.y2) &&
            (self.y2 >= other.y1)
    }
}


#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position(pub i32, pub i32);

impl Position {
    pub fn new(x: i32, y: i32) -> Position {
        Position(x, y)
    }

    pub fn from_pair(pair: (i32, i32)) -> Position {
        Position::new(pair.0, pair.1)
    }

    pub fn distance(&self, other: &Position) -> i32 {
        let dist_i32 = (self.0 - other.0).pow(2) + (self.1 - other.1).pow(2);
        (dist_i32 as f64).sqrt() as i32
    }

    pub fn pair(&self) -> (i32, i32) {
        (self.0, self.1)
    }

    pub fn move_by(&self, dist_x: i32, dist_y: i32) -> Position {
        Position(self.0 + dist_x, self.1 + dist_y)
    }

    pub fn move_x(&self, dist_x: i32) -> Position {
        Position(self.0 + dist_x, self.1)
    }

    pub fn move_y(&self, dist_y: i32) -> Position {
        Position(self.0, self.1 + dist_y)
    }

    pub fn add(&self, other: Position) -> Position{
        Position(self.0 + other.0, self.1 + other.1)
    }

    pub fn into_pair(&self) -> (i32, i32) {
        return (self.0, self.1);
    }
}

impl Into<(i32, i32)> for Position {
    fn into(self) -> (i32, i32) {
        (self.0, self.1)
    }
}
