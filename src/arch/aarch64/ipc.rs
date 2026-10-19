/// Размер очереди сообщений на процесс (8 сообщений).
pub const QUEUE_SIZE: usize = 8;

/// Сообщение — фиксированный размер, 64 байта.
/// tag — тип сообщения (например, 1=write, 2=exit), data — payload.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Message {
    pub tag: u64,
    pub data: [u64; 7],     // 7 * 8 = 56 байт, + tag = 64 байта
}

pub const EMPTY_MSG: Message = Message {
    tag: 0,
    data: [0; 7],
};

/// Ring buffer: до QUEUE_SIZE сообщений, FIFO.
pub struct Queue {
    msgs: [Message; QUEUE_SIZE],
    head: u32,   // куда писать (next write index, обёрнутый)
    tail: u32,   // откуда читать (next read index)
}

pub const EMPTY_QUEUE: Queue = Queue::new();

impl Queue {
    pub const fn new() -> Self {
        Self {
            msgs: [EMPTY_MSG; QUEUE_SIZE],
            head: 0,
            tail: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    pub fn is_full(&self) -> bool {
        // head - tail = QUEUE_SIZE => full
        self.head.wrapping_sub(self.tail) >= QUEUE_SIZE as u32
    }

    /// Положить сообщение. Возвращает false, если очередь полна.
    pub fn push(&mut self, m: Message) -> bool {
        if self.is_full() {
            return false;
        }
        let idx = (self.head as usize) & (QUEUE_SIZE - 1);
        self.msgs[idx] = m;
        self.head = self.head.wrapping_add(1);
        true
    }

    /// Забрать сообщение. Возвращает None, если пусто.
    pub fn pop(&mut self) -> Option<Message> {
        if self.is_empty() {
            return None;
        }
        let idx = (self.tail as usize) & (QUEUE_SIZE - 1);
        let m = self.msgs[idx];
        self.tail = self.tail.wrapping_add(1);
        Some(m)
    }
}