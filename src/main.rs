mod list_devices;

#[cfg(test)]
use common_macros::hash_map;

use clap::{Parser, Subcommand};
use home::home_dir;
use libusb::Context;
use list_devices::{find_for_serial_ids, find_with_libusb};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::BufReader,
    process::Command,
    thread::sleep,
    time::Duration,
};
use udev::Enumerator;

const CONFIG_FILE_PATH: &str = "/.config/layout_switcher/config.json";

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct LayoutSwitcherConfig {
    keyboards: HashMap<String, Vec<String>>,
}

/// Program to change the keyboard layout depending on the
/// usb keyboard that is connected, with either the command line
/// arguments or the config file under the
/// ".config/layout_switcher/config.json" file.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Keyboard serial id.
    #[clap(short, long)]
    keyboard: Option<String>,

    /// Commands to run if the keyboard is connected, in JSON format.
    #[clap(short, long)]
    commands: Option<String>,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    List,
    Monitor,
}

fn run_commands(commands: &[String]) {
    commands.iter().for_each(|command| {
        println!("command: {}", *command);

        println!(
            "{:?}",
            Command::new("sh")
                .arg("-c")
                .arg(command)
                .output()
                .unwrap_or_else(|_| panic!("command {} didn't work", *command))
        );
    });
}

fn parse_args(args: &Args) -> Option<(String, Vec<String>)> {
    match (args.keyboard.as_ref(), args.commands.as_ref()) {
        (Some(keyboard_id), Some(commands)) => Some((
            keyboard_id.to_owned(),
            serde_json::from_str(commands).unwrap(),
        )),
        _ => None,
    }
}

fn get_config_from_file() -> LayoutSwitcherConfig {
    let config_file_str = format!(
        "{}{}",
        home_dir().unwrap().to_str().unwrap(),
        CONFIG_FILE_PATH
    );

    let config_file = File::open(config_file_str).unwrap();
    let reader = BufReader::new(config_file);

    serde_json::from_reader(reader).unwrap()
}

fn list_devices(enumerator: &mut Enumerator) -> HashSet<String> {
    enumerator
        .scan_devices()
        .unwrap()
        .flat_map(|device| {
            device
                .properties()
                .filter(|p| p.name().to_str().unwrap() == "ID_SERIAL")
                .map(|p| p.value().to_str().unwrap().to_string())
                .collect::<Vec<String>>()
        })
        .collect::<HashSet<String>>()
}

fn main() {
    let args = Args::parse();
    let keyboard_commands_args = parse_args(&args);
    let mut enumerator = Enumerator::new().unwrap();

    find_with_libusb();

    match args.command {
        Commands::List => list_devices(&mut enumerator)
            .iter()
            .for_each(|serial_id| println!("{}", serial_id)),
        Commands::Monitor => {
            let layout_config = get_config_with_args(keyboard_commands_args.as_ref());
            let mut prev = "".to_string();
            loop {
                let keyboard_id = find_for_serial_ids(&mut enumerator, &layout_config.keyboards);

                if let Some(keyboard_id) = keyboard_id {
                    if prev != keyboard_id {
                        println!("connected device: {}", keyboard_id);
                        println!("running commands...");
                        run_commands(layout_config.keyboards.get(keyboard_id.as_str()).unwrap());
                        prev = keyboard_id;
                    }
                } else if prev != "default" {
                    println!("no keyboard found, running default commands");
                    run_commands(layout_config.keyboards.get("default").unwrap());
                    prev = "default".to_string();
                }
                sleep(Duration::from_millis(500))
            }
        }
    }

    let context = Context::new();
    for device_list in context.unwrap().devices().iter() {
        device_list.iter().for_each(|device| println!("{}", device.address()))
    }   
    return;

    for device in enumerator.scan_devices().unwrap() {
        println!();
        println!("{:#?}", device);

        println!("  [properties]");
        for property in device.properties() {
            println!("    - {:?} {:?}", property.name(), property.value());
        }

        println!("  [attributes]");
        for attribute in device.attributes() {
            println!("    - {:?} {:?}", attribute.name(), attribute.value());
        }
    }

    /*
    1. Get the list of keyboards, it will be ordered secuentially with priority.
    loop // could loop every 0.5s(?) {
        2. If one of them is connected, run the commands in the config file to change its layout.
        3. Else, do nothing.
    }
    */
}

fn get_config_with_args(
    keyboard_commands_args: Option<&(String, Vec<String>)>,
) -> LayoutSwitcherConfig {
    let mut layout_config = get_config_from_file();
    match keyboard_commands_args {
        Some((keyboard, commands)) => {
            if layout_config.keyboards.contains_key(keyboard) {
                layout_config
                    .keyboards
                    .insert(keyboard.clone(), commands.clone());
            }
            layout_config
        }
        None => layout_config,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_layout_config() {
        let layout_config = LayoutSwitcherConfig {
            keyboards: hash_map! {
                "keyboard_serial_id".to_string() => vec!["echo 'this is so cool'".to_string()],
            },
        };

        assert_eq!(
            "{\"keyboards\":{\"keyboard_serial_id\":[\"echo 'this is so cool'\"]}}",
            serde_json::to_string(&layout_config).unwrap()
        );
    }
}
