use rand::Rng;
use rand::rng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Write};
use std::time::Instant;
/// Конфиг для (де)сериализации через JSON
#[derive(Serialize, Deserialize, Debug)]
struct ConfigData {
    alphabet: String,                 // "latin" или "cyrillic"
    plugboard: Vec<(char, char)>,     // пары замен
    blocks: Vec<String>,              // строки цветовых меток, напр. "КБЧ"
    rotor_positions: Vec<Vec<usize>>, // для каждого блока — вектор стартовых позиций роторов
}
/// Таблица: символ → индекс в алфавите
struct AlphabetIndex {
    min: u32,
    indices: Vec<Option<usize>>,
}
impl AlphabetIndex {
    fn new(alphabet: &[char]) -> Self {
        let codes: Vec<u32> = alphabet.iter().map(|&c| c as u32).collect();
        let &min = codes.iter().min().unwrap();
        let &max = codes.iter().max().unwrap();
        let size = (max - min + 1) as usize;
        let mut indices = vec![None; size];
        for (i, &c) in alphabet.iter().enumerate() {
            indices[(c as u32 - min) as usize] = Some(i);
        }
        AlphabetIndex { min, indices }
    }
    #[inline]
    fn get(&self, c: char) -> Option<usize> {
        let code = c as u32;
        if code < self.min || code > self.min + (self.indices.len() - 1) as u32 {
            None
        } else {
            self.indices[(code - self.min) as usize]
        }
    }
}
/// Ротор (работает с индексами)
struct Rotor {
    shift: usize,
    position: usize,
    size: usize,
}
impl Rotor {
    fn new(shift: usize, alphabet_len: usize) -> Self {
        Rotor {
            shift,
            position: 0,
            size: alphabet_len,
        }
    }
    #[inline]
    fn encode_index(&self, idx: usize, reverse: bool) -> usize {
        if reverse {
            // (idx - (shift + position) + size) % size
            (idx + self.size - ((self.shift + self.position) % self.size)) % self.size
        } else {
            // (idx + shift + position) % size
            (idx + self.shift + self.position) % self.size
        }
    }
    #[inline]
    fn rotate(&mut self) -> bool {
        self.position = (self.position + 1) % self.size;
        self.position == 0
    }
    #[inline]
    fn save_position(&self) -> usize {
        self.position
    }
    #[inline]
    fn load_position(&mut self, pos: usize) {
        self.position = pos % self.size;
    }
}
/// Блок роторов
struct Block {
    rotors: Vec<Rotor>,
}
impl Block {
    fn new(colors: &str, alphabet_len: usize) -> Self {
        let rotors = colors
            .chars()
            .map(|col| {
                let shift = match col {
                    'К' => 1,
                    'Б' => 2,
                    'Ч' => 3,
                    'З' => 5,
                    'Р' => 4,
                    'О' => 6,
                    'Ф' => 7,
                    'С' => 8,
                    'Г' => 9,
                    'Л' => 10,
                    _ => panic!("Неизвестный цвет"),
                };
                Rotor::new(shift, alphabet_len)
            })
            .collect();
        Block { rotors }
    }
    #[inline]
    fn process_index(&self, mut idx: usize, reverse: bool) -> usize {
        if !reverse {
            for r in &self.rotors {
                idx = r.encode_index(idx, false);
            }
        } else {
            for r in self.rotors.iter().rev() {
                idx = r.encode_index(idx, true);
            }
        }
        idx
    }
    fn rotate(&mut self) {
        let mut carry = true;
        for r in &mut self.rotors {
            if carry {
                carry = r.rotate();
            } else {
                break;
            }
        }
    }
    fn save_positions(&self) -> Vec<usize> {
        self.rotors.iter().map(Rotor::save_position).collect()
    }
    fn load_positions(&mut self, pos: &[usize]) {
        for (r, &p) in self.rotors.iter_mut().zip(pos.iter()) {
            r.load_position(p);
        }
    }
}
/// Рефлектор (работает с индексами)
struct Reflector {
    map_idx: Vec<usize>,
}
impl Reflector {
    fn new(alphabet: &[char]) -> Self {
        let len = alphabet.len();
        let mut map_idx = vec![0; len];
        for (i, slot) in map_idx.iter_mut().enumerate() {
            *slot = len - 1 - i;
        }
        Reflector { map_idx }
    }
    #[inline]
    fn reflect_index(&self, idx: usize) -> usize {
        self.map_idx[idx]
    }
}
/// Машина ЭСД
struct EnigmaSudnogoDnya {
    alphabet: Vec<char>,
    index_map: AlphabetIndex,
    plugboard_map: Vec<usize>,
    blocks: Vec<Block>,
    reflector: Reflector,
}
impl EnigmaSudnogoDnya {
    fn new(cfg: &ConfigData) -> Self {
        // 1) Собираем вектор символов алфавита
        let alph_str = if cfg.alphabet == "latin" {
            "abcdefghijklmnopqrstuvwxyz"
        } else {
            "абвгдеёжзийклмнопрстуфхцчшщъыьэюя"
        };
        let alphabet: Vec<char> = alph_str.chars().collect();
        let alphabet_len = alphabet.len();
        // 2) Индекс-таблица: char → Option<usize>
        let index_map = AlphabetIndex::new(&alphabet);
        // 3) Plugboard: строим Vec<usize> длины alphabet_len
        let mut plugboard_map = (0..alphabet_len).collect::<Vec<usize>>();
        for &(a, b) in cfg.plugboard.iter() {
            let ia = index_map.get(a).expect("Символ вне алфавита");
            let ib = index_map.get(b).expect("Символ вне алфавита");
            plugboard_map[ia] = ib;
            plugboard_map[ib] = ia;
        }
        // 4) Создаём блоки роторов
        let mut blocks: Vec<Block> = cfg
            .blocks
            .iter()
            .map(|s| Block::new(s, alphabet_len))
            .collect();
        // 5) Загружаем стартовые позиции из cfg.rotor_positions (если есть)
        if cfg.rotor_positions.len() == blocks.len() {
            for (i, block) in blocks.iter_mut().enumerate() {
                let pos_vec = &cfg.rotor_positions[i];
                block.load_positions(pos_vec);
            }
        } else if !cfg.rotor_positions.is_empty() {
            panic!(
                "Ошибка: rotor_positions.len() ({}) != blocks.len() ({})",
                cfg.rotor_positions.len(),
                blocks.len()
            );
        }
        // 6) Создаём рефлектор
        let reflector = Reflector::new(&alphabet);
        EnigmaSudnogoDnya {
            alphabet,
            index_map,
            plugboard_map,
            blocks,
            reflector,
        }
    }
    fn encrypt(&mut self, msg: &str) -> String {
        // 1) Переводим msg в lowercase (алфавит у нас в нижнем регистре)
        let lower = msg.to_lowercase();
        // 2) Преобразуем в Vec<Option<usize>>
        let mut input_indices: Vec<Option<usize>> = Vec::with_capacity(lower.len());
        for ch in lower.chars() {
            input_indices.push(self.index_map.get(ch));
        }
        // 3) Подготовим выходной буфер индексов
        let mut output_indices: Vec<Option<usize>> = Vec::with_capacity(input_indices.len());
        // 4) Горячий цикл: для каждого символа
        for &maybe_idx in input_indices.iter() {
            if let Some(mut idx) = maybe_idx {
                // plugboard
                idx = self.plugboard_map[idx];
                // через роторы вперёд
                for blk in &self.blocks {
                    idx = blk.process_index(idx, false);
                }
                // рефлектор
                idx = self.reflector.reflect_index(idx);
                // через роторы назад
                for blk in self.blocks.iter().rev() {
                    idx = blk.process_index(idx, true);
                }
                // plugboard обратно
                idx = self.plugboard_map[idx];
                // вращаем все блоки
                for blk in &mut self.blocks {
                    blk.rotate();
                }
                output_indices.push(Some(idx));
            } else {
                // символ вне алфавита
                output_indices.push(None);
            }
        }
        // 5) Собираем итоговую строку
        // Чтобы вернуть точный символ (пробел, цифра или пунктуация),
        // берём символ из lower при None.
        let mut out = String::with_capacity(output_indices.len());
        let mut it_lower = lower.chars();
        for maybe_idx in output_indices.into_iter() {
            if let Some(idx) = maybe_idx {
                out.push(self.alphabet[idx]);
                it_lower.next().unwrap();
            } else {
                let ch = it_lower.next().unwrap();
                out.push(ch);
            }
        }
        out
    }
    fn load_config(filename: &str) -> io::Result<ConfigData> {
        let s = fs::read_to_string(filename)?;
        let cfg = serde_json::from_str(&s)?;
        Ok(cfg)
    }
}
fn read_line() -> String {
    let mut s = String::new();
    io::stdin().read_line(&mut s).unwrap();
    s.trim().to_string()
}
/// Вычисляет log2(n!)
fn log2_factorial(n: usize) -> f64 {
    let mut sum = 0.0;
    for i in 1..=n {
        sum += (i as f64).log2();
    }
    sum
}
fn main() {
    // A) Загрузка или генерация конфига
    let cfg = if fs::metadata("esd_config.json").is_ok() {
        print!("Найден конфиг, загрузить? (да/нет): ");
        io::stdout().flush().unwrap();
        if read_line().to_lowercase() == "да" {
            EnigmaSudnogoDnya::load_config("esd_config.json").expect("Не удалось загрузить")
        } else {
            fs::remove_file("esd_config.json").ok();
            ConfigData {
                alphabet: "latin".into(),
                plugboard: Vec::new(),
                blocks: Vec::new(),
                rotor_positions: Vec::new(),
            }
        }
    } else {
        ConfigData {
            alphabet: "latin".into(),
            plugboard: Vec::new(),
            blocks: Vec::new(),
            rotor_positions: Vec::new(),
        }
    };
    // B) Генерация нового, если cfg.blocks пустой
    let mut cfg = cfg;
    if cfg.blocks.is_empty() {
        // 1) Выбор алфавита
        println!("Выберите алфавит:\n1) Латиница\n2) Кириллица");
        print!("> ");
        io::stdout().flush().unwrap();
        cfg.alphabet = if read_line() == "1" {
            "latin".into()
        } else {
            "cyrillic".into()
        };
        // Подготовим alphabet_chars и alphabet_len
        let alph_str = if cfg.alphabet == "latin" {
            "abcdefghijklmnopqrstuvwxyz"
        } else {
            "абвгдеёжзийклмнопрстуфхцчшщъыьэюя"
        };
        let alphabet_chars: Vec<char> = alph_str.chars().collect();
        let alphabet_len = alphabet_chars.len();
        // 2) Настройка plugboard (ручной/случайный)
        println!("Настройка plugboard (взаимозамен):");
        println!("1) Ввести вручную");
        println!("2) Сгенерировать случайно");
        print!("> ");
        io::stdout().flush().unwrap();
        let pb_choice = read_line();
        let mut plugboard_pairs: Vec<(char, char)> = Vec::new();
        if pb_choice == "1" {
            println!("Вводите пары 'a b' (символы из алфавита). Пустая строка — выход.");
            loop {
                print!("Добавить пару: ");
                io::stdout().flush().unwrap();
                let line = read_line();
                if line.trim().is_empty() {
                    break;
                }
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() != 2 {
                    eprintln!("Нужно ровно два символа через пробел.");
                    continue;
                }
                let a = parts[0].chars().next().unwrap();
                let b = parts[1].chars().next().unwrap();
                if !alphabet_chars.contains(&a) || !alphabet_chars.contains(&b) {
                    eprintln!("Символ не из выбранного алфавита!");
                    continue;
                }
                if a == b {
                    eprintln!("Нельзя ставить пару один и тот же символ.");
                    continue;
                }
                if plugboard_pairs
                    .iter()
                    .any(|&(x, y)| x == a || y == a || x == b || y == b)
                {
                    eprintln!("Один из символов уже задействован.");
                    continue;
                }
                plugboard_pairs.push((a, b));
            }
        } else {
            // Случайная генерация
            let mut rng = rng();
            let mut unused: Vec<char> = alphabet_chars.clone();
            // Перемешаем простейшим образом
            for i in (1..unused.len()).rev() {
                let j = rng.random_range(0..=i);
                unused.swap(i, j);
            }
            // Сделаем примерно половину возможных пар (N/4 пар)
            let pairs_count = alphabet_len / 4;
            for i in 0..pairs_count {
                let a = unused[2 * i];
                let b = unused[2 * i + 1];
                plugboard_pairs.push((a, b));
            }
            println!("Случайно сгенерированные пары plugboard: {:?}", plugboard_pairs);
        }
        cfg.plugboard = plugboard_pairs;
        // 3) Число блоков
        print!("Сколько блоков? ");
        io::stdout().flush().unwrap();
        let n: usize = read_line().parse().unwrap_or(4);
        // 4) Генерируем случайные блоки и позиции
        let mut rng = rng();
        let colors: Vec<char> = vec!['К', 'Б', 'Ч', 'З', 'Р', 'О', 'Ф', 'С', 'Г', 'Л'];
        cfg.blocks = Vec::new();
        cfg.rotor_positions = Vec::new();
        for _ in 0..n {
            // Генерируем длину блока от 3 до 9
            let k = rng.random_range(3..10);
            // Строим строку цветов
            let mut block = String::new();
            for _ in 0..k {
                let idx = rng.random_range(0..colors.len());
                block.push(colors[idx]);
            }
            println!("  блок: {}", block);
            cfg.blocks.push(block.clone());
            // Случайные стартовые позиции для каждого ротора
            let mut positions: Vec<usize> = Vec::new();
            for _ in 0..k {
                let pos = rng.random_range(0..alphabet_len);
                positions.push(pos);
            }
            println!("  стартовые позиции (random): {:?}", positions);
            cfg.rotor_positions.push(positions);
        }
        // 5) Сохранить конфиг?
        print!("Сохранить конфиг? (да/нет): ");
        io::stdout().flush().unwrap();
        if read_line().to_lowercase() == "да" {
            serde_json::to_writer_pretty(fs::File::create("esd_config.json").unwrap(), &cfg)
                .unwrap();
        }
    }
    // D) Основной цикл
    loop {
        print!("Команда (encrypt/decrypt/benchmark/exit): ");
        io::stdout().flush().unwrap();
        match read_line().as_str() {
            "exit" => break,
            "encrypt" => {
                // Шифруем на новой машине в начальном состоянии
                let mut enigma_enc = EnigmaSudnogoDnya::new(&cfg);
                print!("Сообщение: ");
                io::stdout().flush().unwrap();
                let msg = read_line();
                println!("Результат: {}", enigma_enc.encrypt(&msg));
            }
            "decrypt" => {
                // Для дешифрования создаём новую машину из cfg
                let mut enigma_dec = EnigmaSudnogoDnya::new(&cfg);
                print!("Сообщение: ");
                io::stdout().flush().unwrap();
                let msg = read_line();
                println!("Результат: {}", enigma_dec.encrypt(&msg));
            }
            "benchmark" => {
                let mut rng = rng();
                {
                    // A:
                    let alphabet_len = {
                        let tmp = EnigmaSudnogoDnya::new(&cfg);
                        tmp.alphabet.len()
                    };
            
                    // R:
                    let total_rotors: usize = cfg.blocks.iter().map(|blk| blk.len()).sum();
            
                    // P:
                    let plugboard_pairs = cfg.plugboard.len();
            
                    // log2(A^R) = R * log2(A)
                    let log2_positions = (total_rotors as f64) * (alphabet_len as f64).log2();
            
                    // log2(plugboard count) = log2(A!) - log2((A - 2P)!) - P*1 (для 2^P) - log2(P!)
                    let log2_plugboard = log2_factorial(alphabet_len)
                        - log2_factorial(alphabet_len.saturating_sub(2 * plugboard_pairs))
                        - (plugboard_pairs as f64)
                        - log2_factorial(plugboard_pairs);
            
                    let total_bitness = log2_positions + log2_plugboard;
            
                    println!(
                        "\nБитность конфигурации: {:.3} бит (A = {}, R = {}, P = {})",
                        total_bitness, alphabet_len, total_rotors, plugboard_pairs
                    );
                }
                for &size in &[10, 100, 1_000, 10_000, 50_000, 100 * 100 * 100] {
                    // 1) Определим длину алфавита
                    let alphabet_len = {
                        let tmp = EnigmaSudnogoDnya::new(&cfg);
                        tmp.alphabet.len()
                    };
            
                    // 2) Генерируем случайный текст из букв алфавита
                    let mut text = String::with_capacity(size);
                    for _ in 0..size {
                        let idx = rng.random_range(0..alphabet_len);
                        let tmp = EnigmaSudnogoDnya::new(&cfg);
                        text.push(tmp.alphabet[idx]);
                    }
                    // 3) KAT: проверка decrypt(encrypt(text)) == text
                    let t3=Instant::now();
                    let mut enigma_enc = EnigmaSudnogoDnya::new(&cfg);
                    let cipher = enigma_enc.encrypt(&text);
                    let mut enigma_dec = EnigmaSudnogoDnya::new(&cfg);
                    let recovered = enigma_dec.encrypt(&cipher);
                    let kat_time = t3.elapsed().as_secs_f32();
                    if recovered != text {
                        eprintln!(
                            "KAT FAILED на size = {}: decrypt(encrypt(text)) != text",
                            size
                        );
                    } else {
                        println!("KAT: pass");
                    }
                
                    // 4) Измеряем encrypt
                    let mut enigma_for_encrypt = EnigmaSudnogoDnya::new(&cfg);
                    let t0 = Instant::now();
                    let _ = enigma_for_encrypt.encrypt(&text);
                    let enc_t = t0.elapsed().as_secs_f32();
                    // 5) Измеряем decrypt
                    let mut enigma_for_decrypt = EnigmaSudnogoDnya::new(&cfg);
                    let t1 = Instant::now();
                    let _ = enigma_for_decrypt.encrypt(&cipher);
                    let dec_t = t1.elapsed().as_secs_f32();
                    println!("{} → encrypt: {:.6}, decrypt: {:.6}, KAT: {:.6}", size, enc_t, dec_t, kat_time);
                }
            
        
            }
            _ => println!("Неизвестная команда."),
        }
    }
}