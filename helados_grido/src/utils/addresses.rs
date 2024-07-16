use std::net::Ipv4Addr;

pub fn id_to_addr_robot(id: u16) -> (Ipv4Addr, u16) {
    (Ipv4Addr::new(127, 0, 0, 1), 2000 + id)
}

pub fn id_to_addr_screen(id: u16) -> (Ipv4Addr, u16) {
    (Ipv4Addr::new(127, 0, 0, 1), 3000 + id)
}

pub fn id_to_addr_resolver(id: u16) -> (Ipv4Addr, u16) {
    (Ipv4Addr::new(127, 0, 0, 1), 4000 + id)
}

pub fn id_to_addr_leader(id: u16) -> (Ipv4Addr, u16) {
    (Ipv4Addr::new(127, 0, 0, 1), 5000 + id)
}
