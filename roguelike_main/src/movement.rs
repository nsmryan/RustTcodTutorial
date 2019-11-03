use tcod::line::*;

use roguelike_core::map::*;
use roguelike_core::types::*;

use crate::ai::*;
use crate::constants::*;
use crate::game::*;


#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Collision {
    NoCollision(i32, i32),
    BlockedTile((i32, i32), (i32, i32)),
    Wall((i32, i32), (i32, i32)),
    Entity(ObjectId, (i32, i32)),
}

impl Collision {
    pub fn no_collsion(&self) -> bool {
        match self {
            Collision::NoCollision(_, _) => true,
            _ => false,
        }
    }

    pub fn move_location(&self) -> (i32, i32) {
        let last_pos;

        match *self {
            Collision::NoCollision(x, y) => {
                last_pos = (x, y);
            }

            Collision::BlockedTile(_, (x, y)) => {
                last_pos = (x, y);
            }

            Collision::Wall(_, (x, y)) => {
                last_pos = (x, y);
            }

            Collision::Entity(_, (x, y)) => {
                last_pos = (x, y);
            }
        }

        return last_pos;
    }
}


/// Check whether a move, given as an offset from an object's current position,
/// hits a wall or object.
pub fn move_valid(object_id: ObjectId, objects: &[Object], dx: i32, dy: i32, map: &Map) -> bool {
    return check_collision(object_id, objects, dx, dy, map).no_collsion();
}

pub fn line_inclusive(x: i32, y: i32, dx: i32, dy: i32) -> impl Iterator<Item=(i32, i32)> {
    let start_loc = (x + (dx.signum() * dx.abs() * -1),
                     y + (dy.signum() * dy.abs() * -1));
    let end_loc = (x + dx, y + dy);

    return Line::new(start_loc, end_loc).into_iter();
}

pub fn move_just_before(object_id: ObjectId, objects: &[Object], dx: i32, dy: i32, map: &Map) -> Option<(i32, i32)> {
    let x = objects[object_id].x;
    let y = objects[object_id].y;
    let move_line = Line::new((x, y), (x + dx, y + dy));

    let mut pos = None;
    let mut collided = false;

    for (x_pos, y_pos) in move_line.into_iter() {
        if !map.is_within_bounds(x_pos, y_pos) {
            break;
        }

        if is_blocked(map, x_pos, y_pos, objects) ||
           map.is_blocked_by_wall(x_pos, y_pos, dx, dy) {
                collided = true;
                break;
        }

        pos = Some((x_pos, y_pos));
    }

    if !collided {
        pos = None;
    }

    return pos;
}

/// Moves the given object with a given offset, returning the square that it collides with, or None
/// indicating no collision.
pub fn check_collision(object_id: ObjectId,
                       objects: &[Object],
                       dx: i32,
                       dy: i32,
                       map: &Map) -> Collision {
    let x = objects[object_id].x;
    let y = objects[object_id].y;
    let move_line = Line::new((x, y), (x + dx, y + dy));

    let mut last_pos = (x, y);
    let mut result: Collision = Collision::NoCollision(x + dx, y + dy);

    if !map.is_within_bounds(x + dx, y + dy) {
        result = Collision::Wall((x, y), (x, y));
    } else {
        for (x_pos, y_pos) in move_line.into_iter() {
            if is_blocked(map, x_pos, y_pos, objects) {
                if map[(x_pos, y_pos)].blocked {
                    result = Collision::BlockedTile((x_pos, y_pos), last_pos);
                } else {
                    let entity_id = objects.iter()
                                           .enumerate()
                                           .find(|(_index, obj)| obj.pos() == (x_pos, y_pos))
                                           .unwrap()
                                           .0;

                    result = Collision::Entity(entity_id, last_pos);
                }
                break;
            }

            if map.is_blocked_by_wall(x_pos, y_pos, dx, dy) {
                result = Collision::Wall((x_pos + dx, y_pos + dy), (x_pos, y_pos));
                break;
            }

            last_pos = (x_pos, y_pos);
        }
    }

    return result;
}

pub fn player_move_or_attack(move_action: MoveAction,
                         map: &Map,
                         objects: &mut [Object]) -> PlayerAction {
    let player_action: PlayerAction;

    let movement = calculate_move(move_action, objects[PLAYER].movement.unwrap(), PLAYER, objects, map);

    match movement {
        Some(Movement::Attack(new_x, new_y, target_id)) => {
            let (player, target) = mut_two(PLAYER, target_id, objects);
            player.attack(target);

            // if we attack without moving, we lost all our momentum
            if (new_x, new_y) == (objects[PLAYER].x, objects[PLAYER].y)
            {
                objects[PLAYER].momentum.as_mut().map(|momentum| momentum.clear());
            }

            objects[PLAYER].set_pos(new_x, new_y);

            player_action = PlayerAction::TookTurn;
        }

        Some(Movement::Collide(x, y)) => {
            objects[PLAYER].set_pos(x, y);
            objects[PLAYER].momentum.unwrap().clear();
            player_action = PlayerAction::TookTurn;
        }

        Some(Movement::Move(x, y)) | Some(Movement::JumpWall(x, y)) => {
            let (dx, dy) = (x - objects[PLAYER].x, y - objects[PLAYER].y);

            objects[PLAYER].set_pos(x, y);
            let momentum = objects[PLAYER].momentum.unwrap();

            objects[PLAYER].momentum.as_mut().map(|momentum| momentum.moved(dx, dy));

            if momentum.magnitude() > 1 && !momentum.took_half_turn {
                player_action = PlayerAction::TookHalfTurn;
            } else {
                player_action = PlayerAction::TookTurn;
            }

            objects[PLAYER].momentum.as_mut().map(|momentum| momentum.took_half_turn = player_action == PlayerAction::TookHalfTurn);
        }

        Some(Movement::WallKick(x, y, dir_x, dir_y)) => {
            let mut momentum = objects[PLAYER].momentum.unwrap();
            objects[PLAYER].set_pos(x, y);
            momentum.set_momentum(dir_x, dir_y);

            // TODO could check for enemy and attack
            player_action = PlayerAction::TookTurn;
        }

        None => {
            player_action = PlayerAction::DidntTakeTurn;
        }
    }

    return player_action;
}

pub fn calculate_move(action: MoveAction,
                      reach: Reach,
                      object_id: ObjectId,
                      objects: &[Object],
                      map: &Map) -> Option<Movement> {
    let movement: Option<Movement>;

    let (x, y) = objects[object_id].pos();
    if let Some(delta_pos) = reach.move_with_reach(&action) {
        let (dx, dy) = delta_pos.into_pair();
        // check if movement collides with a blocked location or an entity
        match check_collision(object_id, objects, dx, dy, map) {
            Collision::NoCollision(new_x, new_y) => {
                // no collision- just move to location
                movement = Some(Movement::Move(new_x, new_y));
            }

            Collision::BlockedTile((_tile_x, _tile_y), (new_x, new_y)) => {
                movement = Some(Movement::Move(new_x, new_y));
            }

            Collision::Wall((tile_x, tile_y), (new_x, new_y)) => {
                match objects[object_id].momentum {
                    Some(momentum) => {
                        // if max momentum, and there is space beyond the wall, than jump over the wall.
                        if momentum.magnitude() == MAX_MOMENTUM &&
                            !is_blocked(map, tile_x, tile_y, objects) {
                                movement = Some(Movement::JumpWall(tile_x, tile_y));
                        } else { // otherwise move normally, stopping just before the blocking tile
                            movement = Some(Movement::Move(new_x, new_y));
                        }
                    },

                    None => {
                        // with no momentum, the movement will end just before the blocked location
                        movement = Some(Movement::Move(x + dx, y + dy));
                    },
                }
            }

            Collision::Entity(other_object_id, (new_x, new_y)) => {
                // record that an attack would occur. if this is not desired, the
                // calling code will handle this.
                movement = Some(Movement::Attack(new_x, new_y, other_object_id));
            }
        }
    } else {
        movement = None;

    }

    return movement;
}

