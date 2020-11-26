use rand::prelude::*;

use serde::{Serialize, Deserialize};

use logging_timer::timer;

use roguelike_core::types::*;
use roguelike_core::config::*;
use roguelike_core::ai::*;
use roguelike_core::map::*;
use roguelike_core::messaging::{Msg, MsgLog};
use roguelike_core::movement::{Direction, Action};
#[cfg(test)]
use roguelike_core::movement::*;


use crate::actions;
use crate::actions::InputAction; //, KeyDirection};
use crate::generation::*;
use crate::make_map::{make_map, Vault, parse_vault};
use crate::resolve::resolve_messages;
use crate::selection::*;
#[cfg(test)]
use crate::make_map::*;


pub struct Game {
    pub config: Config,
    pub input_action: InputAction,
    pub mouse_state: MouseState,
    pub data: GameData,
    pub settings: GameSettings,
    pub msg_log: MsgLog,
    pub rng: SmallRng,
    pub vaults: Vec<Vault>,
}

impl Game {
    pub fn new(seed: u64, config: Config) -> Result<Game, String> {
        let entities = Entities::new();
        let rng: SmallRng = SeedableRng::seed_from_u64(seed);

        let mut msg_log = MsgLog::new();

        let map = Map::empty();

        let mut data = GameData::new(map, entities);

        let player_id = make_player(&mut data.entities, &config, &mut msg_log);
        data.entities.pos[&player_id] = Pos::new(-1, -1);

        let stone_id = make_stone(&mut data.entities, &config, Pos::new(-1, -1), &mut msg_log);
        data.entities.inventory[&player_id].push_back(stone_id);

        let vaults = Vec::new();

        let state = Game {
            config,
            input_action: InputAction::None,
            data,
            settings: GameSettings::new(0, false),
            mouse_state: Default::default(),
            msg_log,
            rng: rng,
            vaults,
        };

        return Ok(state);
    }

    pub fn load_vaults(&mut self, path: &str) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let vault_file_name = path.to_str().unwrap();
            if !vault_file_name.ends_with(".csv") {
                continue;
            }
            self.vaults.push(parse_vault(vault_file_name, &self.config));
        }
    }

    pub fn step_game(&mut self, dt: f32) -> GameResult {
        self.settings.time += dt;

        let result;
        match self.settings.state {
            GameState::Playing => {
                result = self.step_playing();
            }

            GameState::Win => {
                result = self.step_win();
            }

            GameState::Lose => {
                result = self.step_lose();
            }

            GameState::Inventory => {
                result = self.step_inventory();
            }

            GameState::Selection => {
                result = self.step_selection();
            }

            GameState::SkillMenu => {
                result = self.step_skill_menu();
            }

            GameState::ClassMenu => {
                result = self.step_class_menu();
            }

            GameState::ConfirmQuit => {
                result = self.step_confirm_quit();
            }
        }

        while let Some(msg) = self.msg_log.pop() {
            let msg_line = msg.msg_line(&self.data);
            if msg_line.len() > 0 {
                println!("msg: {}", msg_line);
            }
        }

        return result;
    }

    fn step_win(&mut self) -> GameResult {
        if matches!(self.input_action, InputAction::Exit) {
            return GameResult::Stop;
        }

        self.msg_log.log(Msg::ChangeLevel());

        let player_id = self.data.find_player().unwrap();
        let key_id = self.data.is_in_inventory(player_id, Item::Goal).expect("Won level without goal!");
        self.data.entities.remove_item(player_id, key_id);

        self.settings.state = GameState::Playing;

        self.settings.level_num += 1;

        make_map(&self.config.map_load.clone(), self);

        return GameResult::Continue;
    }

    fn step_lose(&mut self) -> GameResult {
        if self.input_action == InputAction::Exit {
            return GameResult::Stop;
        }

        return GameResult::Continue;
    }

    fn step_inventory(&mut self) -> GameResult {
        let input = self.input_action;
        self.input_action = InputAction::None;

        actions::handle_input_inventory(input, &mut self.data, &mut self.settings, &mut self.msg_log);

        if self.settings.exiting {
            return GameResult::Stop;
        }

        return GameResult::Continue;
    }

    fn step_skill_menu(&mut self) -> GameResult {
        let input = self.input_action;
        self.input_action = InputAction::None;

        let player_action =
            actions::handle_input_skill_menu(input, &mut self.data, &mut self.settings, &mut self.msg_log);

        if player_action != Action::NoAction {
            let win = step_logic(self, player_action);

            if win {
                self.settings.state = GameState::Win;
            }
        }

        if self.settings.exiting {
            return GameResult::Stop;
        }

        return GameResult::Continue;
    }

    fn step_class_menu(&mut self) -> GameResult {
        let input = self.input_action;
        self.input_action = InputAction::None;

        let player_action =
            actions::handle_input_class_menu(input, &mut self.data, &mut self.settings, &mut self.msg_log);

        if player_action != Action::NoAction {
            let win = step_logic(self, player_action);

            if win {
                self.settings.state = GameState::Win;
            }
        }

        if self.settings.exiting {
            return GameResult::Stop;
        }

        return GameResult::Continue;
    }

    fn step_confirm_quit(&mut self) -> GameResult {
        let input = self.input_action;
        self.input_action = InputAction::None;

        actions::handle_input_confirm_quit(input, &mut self.data, &mut self.settings, &mut self.msg_log);

        if self.settings.exiting {
            return GameResult::Stop;
        }

        return GameResult::Continue;
    }

    fn step_selection(&mut self) -> GameResult {
        let input = self.input_action;
        self.input_action = InputAction::None;

        self.settings.draw_selection_overlay = true;

        let player_action =
            actions::handle_input_selection(input,
                                           &mut self.data,
                                           &mut self.settings,
                                           &self.config,
                                           &mut self.msg_log);

        if player_action != Action::NoAction {
            let win = step_logic(self, player_action);
            if win {
                self.settings.state = GameState::Win;
            }
        }

        if self.settings.exiting {
            return GameResult::Stop;
        }

        return GameResult::Continue;
    }

//    fn step_console(&mut self) -> GameResult {
//        let input = self.input_action;
//        self.input_action = InputAction::None;
//
//        let time_since_open = self.settings.time - self.console.time_at_open;
//        let lerp_amount = clampf(time_since_open / self.config.console_speed, 0.0, 1.0);
//        self.console.height = lerp(self.console.height as f32,
//                                   self.config.console_max_height as f32,
//                                   lerp_amount) as u32;
//        if (self.console.height as i32 - self.config.console_max_height as i32).abs() < 2 {
//            self.console.height = self.config.console_max_height;
//        }
//
//        if self.key_input.len() > 0 {
//            // TODO add console back in
//            //actions::handle_input_console(input,
//            //                              &mut self.key_input,
//            //                              &mut self.console,
//            //                              &mut self.data,
//            //                              &mut self.display,
//            //                              &mut self.settings,
//            //                              &self.config,
//            //                              &mut self.msg_log);
//        }
//
//        return GameResult::Continue;
//    }

    fn step_playing(&mut self) -> GameResult {
        let player_action =
            actions::handle_input(self);

        if player_action != Action::NoAction {
            let win = step_logic(self, player_action);
            if win {
                self.settings.state = GameState::Win;
            }
        }

        if self.settings.exiting {
            return GameResult::Stop;
        }

        self.input_action = InputAction::None;

        return GameResult::Continue;
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum GameResult {
    Continue,
    Stop,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSettings {
    pub turn_count: usize,
    pub god_mode: bool,
    pub map_type: MapGenType,
    pub exiting: bool,
    pub state: GameState,
    pub draw_selection_overlay: bool,
    pub overlay: bool,
    pub console: bool,
    pub time: f32,
    pub render_map: bool,
    pub selection: Selection,
    pub inventory_action: InventoryAction,
    pub level_num: usize,
    pub running: bool,
}

impl GameSettings {
    pub fn new(turn_count: usize,
               god_mode: bool) -> GameSettings {
        return GameSettings {
            turn_count,
            god_mode,
            map_type: MapGenType::Island,
            exiting: false,
            state: GameState::Playing,
            draw_selection_overlay: false,
            overlay: false,
            console: false,
            time: 0.0,
            render_map: true,
            selection: Selection::default(),
            inventory_action: InventoryAction::default(),
            level_num: 0,
            running: true,
        };
    }
}

/// Check whether the exit condition for the game is met.
fn level_exit_condition_met(data: &GameData) -> bool {
    // loop over objects in inventory, and check whether any
    // are the key object.
    let player_id = data.find_player().unwrap();
    let player_pos = data.entities.pos[&player_id];

    let mut exit_condition = false;
    if let Some(exit_id) = data.find_exit() {
        let exit_pos = data.entities.pos[&exit_id];

        let has_key = data.is_in_inventory(player_id, Item::Goal).is_some();

        //let on_exit_tile = data.map[player_pos].tile_type == TileType::Exit;
        let on_exit_tile = exit_pos == player_pos;

        exit_condition = has_key && on_exit_tile;
    }

    return exit_condition;
}

pub fn step_logic(game: &mut Game, player_action: Action) -> bool {
    game.msg_log.clear();

    let player_id = game.data.find_player().unwrap();

    game.data.entities.action[&player_id] = player_action;

    /* Actions */
    game.msg_log.log(Msg::Action(player_id, player_action));

    eprintln!();
    eprintln!("Turn {}:", game.settings.turn_count);

    resolve_messages(&mut game.data, &mut game.msg_log, &mut game.settings, &mut game.rng, &game.config);

    let won_level = level_exit_condition_met(&game.data);

    // resolve enemy action
    let monster = timer!("MONSTER");
    if player_action.takes_turn() && game.data.entities.status[&player_id].alive && !won_level {
        let mut ai_id: Vec<EntityId> = Vec::new();

        for key in game.data.entities.ids.iter() {
            if game.data.entities.ai.get(key).is_some()    &&
               game.data.entities.status[key].alive         &&
               game.data.entities.limbo.get(key).is_none() &&
               game.data.entities.fighter.get(key).is_some() {
               ai_id.push(*key);
           }
        }

        for key in ai_id.iter() {
           let action = ai_take_turn(*key, &mut game.data, &game.config, &mut game.msg_log);
           game.data.entities.action[key] = action;

           // if changing state, resolve now and allow another action
           if matches!(action, Action::StateChange(_)) {
                game.msg_log.log(Msg::Action(*key, action));
                resolve_messages(&mut game.data, &mut game.msg_log, &mut game.settings, &mut game.rng, &game.config);
                let backup_action = ai_take_turn(*key, &mut game.data, &game.config, &mut game.msg_log);
                game.data.entities.action[key] = backup_action;
            }
        }

        for key in ai_id.iter() {
            if let Some(action) = game.data.entities.action.get(key).map(|v| *v) {
                game.msg_log.log(Msg::Action(*key, action));
                resolve_messages(&mut game.data, &mut game.msg_log, &mut game.settings, &mut game.rng, &game.config);

                // check if fighter needs to be removed
                if let Some(fighter) = game.data.entities.fighter.get(key) {
                    if fighter.hp <= 0 {
                        game.data.entities.status[key].alive = false;
                        game.data.entities.blocks[key] = false;
                        game.data.entities.chr[key] = '%';
                        game.data.entities.fighter.remove(key);
                    }
                }
            }
        }

        for key in ai_id.iter() {
            // if there are remaining messages for an entity, clear them
            game.data.entities.messages[key].clear();

            let action = ai_take_turn(*key, &mut game.data, &game.config, &mut game.msg_log);
            if matches!(action, Action::StateChange(_)) {
                game.msg_log.log(Msg::Action(*key, action));
                game.data.entities.action[key] = action;
                resolve_messages(&mut game.data, &mut game.msg_log, &mut game.settings, &mut game.rng, &game.config);
            }
        }
    }
    drop(monster);

    // send player turn action in case there is cleanup to perform, or another system
    // needs to know that the turn is finished.
    game.msg_log.log(Msg::PlayerTurn());
    resolve_messages(&mut game.data, &mut game.msg_log, &mut game.settings, &mut game.rng, &game.config);

    let mut to_remove: Vec<EntityId> = Vec::new();

    // check status effects
    for entity_id in game.data.entities.ids.iter() {
        if let Some(mut status) = game.data.entities.status.get_mut(entity_id) {
            if status.frozen > 0 {
                status.frozen -= 1;
            }
        }
    }

    // perform count down
    for entity_id in game.data.entities.ids.iter() {
        if let Some(ref mut count) = game.data.entities.count_down.get_mut(entity_id) {
            if **count == 0 {
                to_remove.push(*entity_id);
            } else {
                **count -= 1;
            }
        }

        if game.data.entities.needs_removal[entity_id] &&
           game.data.entities.animation[entity_id].len() == 0 {
            to_remove.push(*entity_id);
        }
    }

    // remove objects waiting removal
    for key in to_remove {
        game.data.remove_entity(key);
    }

    if player_action.takes_turn() {
        game.settings.turn_count += 1;
    }

    return level_exit_condition_met(&game.data);
}

#[test]
pub fn test_game_step() {
    let mut config = Config::from_file("../config.yaml");
    config.map_load = MapLoadConfig::Empty;
    let mut game = Game::new(0, config.clone()).unwrap();

    let player_id = game.data.find_player().unwrap();
    make_map(&MapLoadConfig::Empty, &mut game);
    let player_pos = game.data.entities.pos[&player_id];
    assert_eq!(Pos::new(0, 0), player_pos);

    game.input_action = InputAction::Move(Direction::Right);
    game.step_game(0.1);
    let player_pos = game.data.entities.pos[&player_id];
    assert_eq!(Pos::new(1, 0), player_pos);

    game.input_action = InputAction::Move(Direction::Down);
    game.step_game(0.1);
    let player_pos = game.data.entities.pos[&player_id];
    assert_eq!(Pos::new(1, 1), player_pos);

    game.input_action = InputAction::Move(Direction::Left);
    game.step_game(0.1);
    let player_pos = game.data.entities.pos[&player_id];
    assert_eq!(Pos::new(0, 1), player_pos);

    game.input_action = InputAction::Move(Direction::Up);
    game.step_game(0.1);
    let player_pos = game.data.entities.pos[&player_id];
    assert_eq!(Pos::new(0, 0), player_pos);
}

// TODO issue 151 removes walking and 150 removes pushing
//      so this test no longer makes any sense.
pub fn test_running() {
    let config = Config::from_file("../config.yaml");
    let mut game = Game::new(0, config.clone()).unwrap();

    let player_id = game.data.find_player().unwrap();
    game.data.map = Map::from_dims(10, 10);
    let player_pos = Pos::new(4, 4);
    game.data.entities.pos[&player_id] = player_pos;

    let gol_pos = Pos::new(4, 5);
    let gol = make_gol(&mut game.data.entities, &game.config, gol_pos, &mut game.msg_log);

    game.data.map[(4, 6)].block_move = true;

    // check that running into a monster crushes it against a wall when no empty tiles
    // between
    game.input_action = InputAction::IncreaseMoveMode;
    game.step_game(0.1);

    assert!(game.data.entities.ids.contains(&gol));
    game.input_action = InputAction::Move(Direction::Down);
    game.step_game(0.1);
    let player_pos = game.data.entities.pos[&player_id];
    assert_eq!(gol_pos, player_pos);

    // gol is no longer in entities list after being crushed
    assert!(!game.data.entities.ids.contains(&gol));

    // check that running into a monster, with water 2 tiles away, pushes monster
    // up to the water
    let pawn_pos = Pos::new(5, 5);
    let pawn = make_pawn(&mut game.data.entities, &game.config, pawn_pos, &mut game.msg_log);

    game.data.map[(7, 5)].tile_type = TileType::Water;

    game.input_action = InputAction::Move(Direction::Right);
    game.step_game(0.1);
    assert_eq!(Pos::new(5, 5), game.data.entities.pos[&player_id]);
    assert_eq!(Pos::new(6, 5), game.data.entities.pos[&pawn]);
}

#[test]
pub fn test_hammer_small_wall() {
    let config = Config::from_file("../config.yaml");
    let mut game = Game::new(0, config.clone()).unwrap();

    let player_id = game.data.find_player().unwrap();
    game.data.map = Map::from_dims(10, 10);
    let player_pos = Pos::new(4, 4);
    game.data.entities.pos[&player_id] = player_pos;


    game.data.map[player_pos].bottom_wall = Wall::ShortWall;

    let gol_pos = Pos::new(4, 5);
    let gol = make_gol(&mut game.data.entities, &game.config, gol_pos, &mut game.msg_log);

    let hammer = make_hammer(&mut game.data.entities, &game.config, Pos::new(4, 7), &mut game.msg_log);

    game.data.entities.inventory[&player_id].push_front(hammer);

    game.input_action = InputAction::UseItem;
    game.step_game(0.1);

    game.input_action = InputAction::MapClick(gol_pos, gol_pos);
    game.step_game(0.1);

    for msg in game.msg_log.turn_messages.iter() {
        println!("{:?}", msg);
    }

    // gol is no longer in entities list after being crushed
    assert!(game.data.entities.is_dead(gol));

    assert!(game.msg_log.turn_messages.iter().any(|msg| {
        matches!(msg, Msg::HammerHitWall(player_id, _))
    }));

    assert_eq!(Surface::Rubble, game.data.map[gol_pos].surface);

    let pawn_pos = Pos::new(3, 4);
    let pawn = make_pawn(&mut game.data.entities, &game.config, pawn_pos, &mut game.msg_log);
    assert_eq!(true, game.data.entities.status[&pawn].alive);

    // add the hammer back and hit the pawn with it to test hitting entities
    let hammer = make_hammer(&mut game.data.entities, &game.config, Pos::new(4, 7), &mut game.msg_log);
    game.data.entities.inventory[&player_id].push_front(hammer);

    game.input_action = InputAction::UseItem;
    game.step_game(0.1);

    game.input_action = InputAction::MapClick(pawn_pos, pawn_pos);
    game.step_game(0.1);

    assert!(game.data.entities.is_dead(pawn));

    assert!(game.msg_log.turn_messages.iter().any(|msg| {
        matches!(msg, Msg::HammerHitEntity(player_id, pawn))
    }));

    assert_ne!(Surface::Rubble, game.data.map[pawn_pos].surface);
}

