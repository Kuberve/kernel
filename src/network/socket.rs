pub const MAX_SOCKETS: usize = 8;
pub const SOCKET_QUEUE: usize = 4;
pub const SOCKET_PKT_MAX: usize = 1024;

#[derive(Clone, Copy)]
pub struct Packet {
    pub data: [u8; SOCKET_PKT_MAX],
    pub len: u16,
    pub src_ip: [u8; 4],
    pub src_port: u16,
}

const EMPTY_PKT: Packet = Packet {
    data: [0; SOCKET_PKT_MAX],
    len: 0,
    src_ip: [0; 4],
    src_port: 0,
};

pub struct Socket {
    pub used: bool,
    pub bound_port: u16,
    pub incoming: [Packet; SOCKET_QUEUE],
    pub head: u32,
    pub tail: u32,
}

const EMPTY_SOCKET: Socket = Socket {
    used: false,
    bound_port: 0,
    incoming: [EMPTY_PKT; SOCKET_QUEUE],
    head: 0,
    tail: 0,
};

static mut SOCKETS: [Socket; MAX_SOCKETS] = [EMPTY_SOCKET; MAX_SOCKETS];

pub fn init() {
    unsafe {
        for i in 0..MAX_SOCKETS {
            SOCKETS[i] = EMPTY_SOCKET;
        }
    }
}

/// Создать сокет. Возвращает id (0..MAX_SOCKETS) или None.
pub unsafe fn create() -> Option<usize> {
    for i in 0..MAX_SOCKETS {
        if !SOCKETS[i].used {
            SOCKETS[i] = EMPTY_SOCKET;
            SOCKETS[i].used = true;
            return Some(i);
        }
    }
    None
}

pub unsafe fn bind(id: usize, port: u16) -> bool {
    if id >= MAX_SOCKETS || !SOCKETS[id].used { return false; }
    SOCKETS[id].bound_port = port;
    true
}

pub unsafe fn close(id: usize) {
    if id < MAX_SOCKETS {
        SOCKETS[id] = EMPTY_SOCKET;
    }
}

pub unsafe fn bound_port(id: usize) -> u16 {
    if id >= MAX_SOCKETS { return 0; }
    SOCKETS[id].bound_port
}

/// Найти сокет по dst_port. Возвращает id или None.
pub unsafe fn find_by_port(port: u16) -> Option<usize> {
    for i in 0..MAX_SOCKETS {
        if SOCKETS[i].used && SOCKETS[i].bound_port == port {
            return Some(i);
        }
    }
    None
}

/// Положить пакет в очередь сокета. Возвращает false, если полная.
pub unsafe fn enqueue(id: usize, pkt: Packet) -> bool {
    if id >= MAX_SOCKETS || !SOCKETS[id].used { return false; }
    let s = &mut SOCKETS[id];
    let used = s.tail.wrapping_sub(s.head);
    if used as usize >= SOCKET_QUEUE { return false; }
    let idx = (s.tail as usize) % SOCKET_QUEUE;
    s.incoming[idx] = pkt;
    s.tail = s.tail.wrapping_add(1);
    true
}

/// Забрать пакет из очереди. None, если пусто.
pub unsafe fn dequeue(id: usize) -> Option<Packet> {
    if id >= MAX_SOCKETS || !SOCKETS[id].used { return None; }
    let s = &mut SOCKETS[id];
    if s.head == s.tail { return None; }
    let idx = (s.head as usize) % SOCKET_QUEUE;
    let pkt = s.incoming[idx];
    s.head = s.head.wrapping_add(1);
    Some(pkt)
}