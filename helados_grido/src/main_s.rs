use helados_grido::screen_lib::screen::Screen;
use std::env;

fn main() {
    env_logger::init();
    log::info!("Starting Screen...");

    // Obtener los argumentos de la línea de comandos
    let args: Vec<String> = env::args().collect();

    // Verificar que se hayan pasado suficientes argumentos
    if args.len() != 5 {
        println!(
            "Uso: {} <id> <numeros_separados_por_dos_puntos> <ruta_al_archivo> <cant_robots>",
            args[0]
        );
        std::process::exit(1);
    }

    // Parsear el ID
    let id: u8 = args[1].parse().expect("El ID no es un número válido");

    // Parsear el vector de números
    let peers: Vec<u8> = args[2]
        .split(':')
        .map(|s| s.parse().expect("No es un número válido"))
        .collect();

    // La ruta del archivo es el tercer argumento
    let path = args[3].clone();

    // La cantidad de robots es el cuarto argumento
    let nrobots: u8 = args[4]
        .parse()
        .expect("La cantidad de robots no es un número válido");

    let mut screen = match Screen::new(id, peers, path, nrobots) {
        Ok(screen) => screen,
        Err(error) => {
            eprintln!("ERROR: {}", error);
            return;
        }
    };

    match screen.start() {
        Ok(_) => {}
        Err(error) => {
            eprintln!("ERROR: {}", error);
        }
    }
}
