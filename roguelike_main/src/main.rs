#![allow(dead_code)]
mod throttler;
mod render;
mod display;
mod keyboard;
mod load;
mod replay;
mod animation;

use std::fs;
use std::io::{BufRead, Write};
use std::time::{Duration, Instant, SystemTime};
use std::path::Path;
use std::str::FromStr;
use std::thread;
use std::sync::mpsc::{self, Receiver};

use log::LevelFilter;
use simple_logging;
use logging_timer::timer;

use gumdrop::Options;

use roguelike_core::types::*;
use roguelike_core::config::Config;
use roguelike_core::constants::*;
use roguelike_core::map::MapLoadConfig;

use roguelike_engine::game::*;
use roguelike_engine::generation::*;
use roguelike_engine::actions::*;
use roguelike_engine::input::*;
use roguelike_engine::make_map::{make_map, read_map_xp};
use roguelike_engine::log::*;

use roguelike_lib::commands::*;

use crate::throttler::*;
use crate::render::*;
use crate::display::*;
use crate::load::*;
use crate::replay::*;


pub const CONFIG_NAME: &str = "config.yaml";


#[derive(Debug, Clone, Options)]
pub struct GameOptions {
    #[options(help = "replay from an input log file", short="r")]
    pub replay: Option<String>,

    #[options(help = "record a session with the given name", short="d")]
    pub record: Option<String>,

    #[options(help = "re-record a session with the given name", short="o")]
    pub rerecord: Option<String>,

    #[options(help = "check a previous recorded session against current version", short="c")]
    pub check: Option<String>,

    #[options(help = "delay value in milliseconds used when replaying commands", short="y")]
    pub delay: Option<u64>,

    #[options(help = "load using the given map configuration", short="m")]
    pub map_config: Option<String>,

    #[options(help = "log level to record in game.log (OFF, ERROR, WARN, INFO, DEBUG, TRACE)")]
    pub log_level: Option<String>,

    #[options(help = "use a given seed for random number generation")]
    pub seed: Option<u64>,

    #[options(help = "take a screenshot and exit", short="t")]
    pub screenshot: bool,

    #[options(help = "procgen map config", short="g")]
    pub procgen_map: Option<String>,

    #[options(help = "display help text")]
    pub help: bool,
}


fn main() {
    let opts = GameOptions::parse_args_default_or_exit();

    let seed: u64 =
        if let Some(given_seed) = opts.seed {
            given_seed
        } else {
            1
            // could add string input as a seed generator
            // let mut hasher = DefaultHasher::new();
            // args[1].hash(&mut hasher);
            // hasher.finish()
        };

    eprintln!("Seed: {} (0x{:X})", seed, seed);

    let log_level =
        opts.log_level.clone().map_or(LevelFilter::Off,
                                      |level_str| LevelFilter::from_str(&level_str).expect("Log level unexpected!"));
    simple_logging::log_to_file("game.log", log_level).unwrap();

    run(seed, opts).unwrap();
}

pub fn run(seed: u64, opts: GameOptions) -> Result<(), String> {
    /* Create SDL Context */
    let sdl_context = sdl2::init()?;
    let video = sdl_context.video()?;
    let window = video.window("Rust Roguelike", SCREEN_WIDTH, SCREEN_HEIGHT)
                      .position_centered().build().map_err(|e| e.to_string())?;

    let canvas = window.into_canvas()
                       .accelerated()
                       .build()
                       .map_err(|e| e.to_string())?;
    let texture_creator = canvas.texture_creator();

    /* Create Display Structures */
    let mut display = Display::new(canvas);

    /* Load Textures */
    load_sprites(&texture_creator, &mut display);
    load_sprite(&texture_creator, &mut display, "resources/rustrogueliketiles.png", "tiles");
    load_sprite(&texture_creator, &mut display, "resources/shadowtiles.png", "shadows");

    let ttf_context = sdl2::ttf::init().expect("Could not init SDL2 TTF!");
    let font_texture = load_font(&ttf_context,
                                 &texture_creator,
                                 &mut display.targets.canvas_panel.target,
                                 "Monoid.ttf".to_string(),
                                 16);
    display.add_spritesheet("font".to_string(), font_texture);

    /* Create Game Structure */
    let config = Config::from_file(CONFIG_NAME);
    let mut game = Game::new(seed, config.clone());

    game.load_vaults("resources/vaults/");

    make_mouse(&mut game.data.entities, &game.config, &mut game.msg_log);

    /* Create Map */
    let mut map_config: MapLoadConfig;

    if let Some(procgen_map) = opts.procgen_map.clone() {
        map_config = MapLoadConfig::ProcGen(procgen_map);
    } else {
        map_config = config.map_load.clone();
    }

    if let Some(map_config_str) = &opts.map_config {
        let cli_map_config = map_config_str.parse::<MapLoadConfig>()
                                           .expect("Could not parse map config option!");
        map_config = cli_map_config;
    }

    // save map config to a file
    let mut map_config_file = std::fs::File::create(MAP_CONFIG_NAME).unwrap();
    map_config_file.write_all(map_config.to_string().as_bytes()).unwrap();

    /* Run Game or Take Screenshot */
    if opts.screenshot {
        make_map(&map_config, &mut game);
        take_screenshot(&mut game, &mut display).unwrap();
        return Ok(());
    } else if let Some(record_name) = opts.check {
        let delay = opts.delay.unwrap_or(0);
        let mut event_pump = sdl_context.event_pump().unwrap();

        if record_name == "all" {
            return check_all_records(&mut game, &mut display, &mut event_pump, delay);
        } else {
            return check_single_record(&mut game, &mut display, &mut event_pump, &record_name, delay);
        }
    } else if let Some(record_name) = opts.rerecord {
        let delay = opts.delay.unwrap_or(0);
        let mut event_pump = sdl_context.event_pump().unwrap();
        if record_name == "all" {
            return rerecord_all(&mut game, &mut display, &mut event_pump, delay);
        } else {
            return rerecord_single(&mut game, &mut display, &mut event_pump, &record_name, delay);
        }
    } else {
        make_map(&map_config, &mut game);
        let event_pump = sdl_context.event_pump().unwrap();
        return game_loop(game, display, opts, event_pump);
    }
}

pub fn game_loop(mut game: Game, mut display: Display, opts: GameOptions, mut event_pump: sdl2::EventPump) -> Result<(), String> {
    // read in the recorded action log, if one is provided
    let mut starting_actions = Vec::new();
    if let Some(replay_file) = &opts.replay {
        starting_actions = read_action_log(&replay_file);
    }

    let mut config_modified_time = fs::metadata(CONFIG_NAME).unwrap().modified().unwrap();

    let mut log = Log::new();
    let mut recording = Recording::new(&game);

    /* Setup FPS Throttling */
    let frame_ms = 1000 / game.config.frame_rate as u64;
    let fps_throttler = Throttler::new(Duration::from_millis(frame_ms));

    /* Set up Input Handling */
    let io_recv = spawn_input_reader();

    /* Main Game Loop */
    let mut frame_time = Instant::now();
    while game.settings.running {
        let _loop_timer = timer!("GAME_LOOP");

        /* Input */
        let mut input_action: InputAction = InputAction::None;
        {
            let _input_timer = timer!("INPUT");

            // check for commands to execute
            process_commands(&io_recv, &mut game, &mut log);

            //input_action = process_input_events(frame_time, &mut event_pump, &mut game, &mut display);
            for sdl2_event in event_pump.poll_iter() {
                if let Some(event) = keyboard::translate_event(sdl2_event, &mut game, &mut display) {
                    if game.config.recording && matches!(event, InputEvent::Char('[', KeyDir::Up)) {
                        game = recording.backward();
                    } else if game.config.recording && matches!(event, InputEvent::Char(']', KeyDir::Up)) {
                        if let Some(new_game) = recording.forward() {
                            game = new_game;
                        }
                    } else {
                        // NOTE may lose inputs if multiple events create actions!
                        input_action = game.input.handle_event(&mut game.settings, event, frame_time, &game.config);
                    }
                }
            }
        }

        /* Misc */
        {
            let _misc_timer = timer!("MISC");

            // if there are starting actions to read, pop one off to play
            if let Some(action) = starting_actions.pop() {
                input_action = action;
            }

           /* Record Inputs to Log File */
           log.log_action(input_action);
        }

        /* Logic */
        {
            let _logic_timer = timer!("LOGIC");
            let dt = Instant::now().duration_since(frame_time).as_secs_f32();
            frame_time = Instant::now();
            game.step_game(input_action, dt);
            
            if game.config.recording && input_action != InputAction::None {
                recording.action(&game, input_action);
            }

            for msg_index in 0..game.msg_log.turn_messages.len() {
                let msg = game.msg_log.turn_messages[msg_index];
                let msg_line = &msg.msg_line(&game.data);
                if msg_line.len() > 0 {
                    log.log_console(msg_line);
                }
                log.log_msg(&format!("{}", msg));
            }

            if game.settings.state == GameState::Win {
                display.clear_level_state();
                recording.clear();
            } else if game.settings.state == GameState::Exit {
                game.settings.running = false;
            }
        }

        /* Display */
        {
            let _display_timer = timer!("DISPLAY");
            update_display(&mut game, &mut display)?;
        }

        game.msg_log.clear();

        /* Configuration */
        {
            let _config_timer = timer!("CONFIG");
            reload_config(&mut config_modified_time, &mut game);
        }

        /* Wait until the next tick to loop */
        {
            let _wait_timer = timer!("WAIT");
            fps_throttler.wait();
        }
    }

    // NOTE we could also just put these files in the right place to begin with...
    if let Some(record_name) = opts.record {
        // save recorded logs
        save_record(&record_name);
    }

    return Ok(());
}

fn save_record(record_name: &str) {
    // create log directory if it doesn't exist
    let path = format!("resources/test_logs/{}", record_name);
    std::fs::create_dir_all(&path).expect("Could not create record directory!");

    // save all files to the new directory 
    std::fs::copy(Log::ACTION_LOG_NAME, format!("{}/{}", &path, Log::ACTION_LOG_NAME))
            .expect("Could not save action log!");

    std::fs::copy(Log::MESSAGE_LOG_NAME, format!("{}/{}", &path, Log::MESSAGE_LOG_NAME))
            .expect("Could not save message log!");

    std::fs::copy(MAP_CONFIG_NAME, format!("{}/{}", &path, MAP_CONFIG_NAME))
            .expect("Could not save map config!");
}

fn reload_config(config_modified_time: &mut SystemTime, game: &mut Game) {
    /* Reload map if configured to do so */
    if game.config.load_map_file_every_frame && Path::new("resources/map.xp").exists() {
        let player = game.data.find_by_name(EntityName::Player).unwrap();

        let map_file = format!("resources/{}", game.config.map_file);
        game.data.entities.clear();
        let player_pos = read_map_xp(&game.config, &mut game.data, &mut game.msg_log, &map_file);
        game.data.entities.set_pos(player, Pos::from(player_pos));
    }

    /* Reload Configuration */
    if let Ok(current_config_modified_time) = fs::metadata(CONFIG_NAME) {
        let current_config_modified_time = current_config_modified_time.modified().unwrap();
        if current_config_modified_time != *config_modified_time {
            *config_modified_time = current_config_modified_time;
            game.config = Config::from_file(CONFIG_NAME);
        }
    }
}

pub fn take_screenshot(game: &mut Game, display: &mut Display) -> Result<(), String> {
    game.settings.god_mode = true;

    game.step_game(InputAction::None, 0.0);
    render_all(display, game)?;

    display.save_screenshot("screenshot");

    return Ok(());
}

fn update_display(game: &mut Game, display: &mut Display) -> Result<(), String> {
    for msg in game.msg_log.turn_messages.iter() {
        display.process_message(*msg, &mut game.data, &game.config);
    }

    /* Draw the Game to the Screen */
    render_all(display, game)?;

    display.update_display();

    return Ok(());
}

//fn process_input_events(frame_time: Instant, event_pump: &mut EventPump, game: &mut Game, display: &mut Display) -> InputAction {
//    let mut input_action: InputAction = InputAction::None;
//
//    for sdl2_event in event_pump.poll_iter() {
//        if let Some(event) = keyboard::translate_event(sdl2_event, game, display) {
//            let action = game.input.handle_event(&mut game.settings, event, frame_time, &game.config);
//            // NOTE may lose inputs if multiple events create actions!
//            input_action = action;
//        }
//    }
//
//    return input_action;
//}

fn process_commands(io_recv: &Receiver<String>, game: &mut Game, log: &mut Log) {
    if let Ok(msg) = io_recv.recv_timeout(Duration::from_millis(0)) {
        if let Ok(cmd) = msg.parse::<GameCmd>() {
            let result = execute_game_command(&cmd, game);
            if !result.is_empty() {
                log.log_output(&result);
            }
        } else {
            log.log_output(&format!("error '{}' unexpected", msg));
        }
    }
}

fn spawn_input_reader() -> Receiver<String> {
    let (io_send, io_recv) = mpsc::channel();

    thread::spawn(move || {
        let stdin = std::io::stdin();
        let stdin = stdin.lock().lines();

        for line in stdin {
            let text = line.unwrap();
            if !text.is_empty() {
                io_send.send(text).unwrap();
            }
        }
    });

    return io_recv;
}

