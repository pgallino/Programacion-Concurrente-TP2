use helados_grido::utils::addresses::id_to_addr_robot;
use std::{env, net::Ipv4Addr};

use helados_grido::robot_lib::robot::Robot;

fn main() {
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        println!(
            "Uso: {} <id> <path separado por ':' iniciando por <id>>",
            args[0]
        );
        std::process::exit(1);
    }
    let id: u8 = args[1].parse::<u8>().unwrap();
    let my_addr: (Ipv4Addr, u16) = id_to_addr_robot(id as u16);
    let peers: Vec<u8> = args[2]
        .split(':')
        .map(|s| s.parse().expect("No es un número válido"))
        .collect();

    let mut robot = match Robot::new(id, peers, my_addr) {
        Ok(robot) => robot,
        Err(error) => {
            eprintln!("ERROR: {}", error);
            return;
        }
    };

    // thread::sleep(Duration::from_secs(10));

    match robot.start() {
        Ok(_) => {}
        Err(error) => {
            eprintln!("ERROR: {}", error);
        }
    }
    // env_logger::init();
    // log::info!("Starting Robot...");
}
