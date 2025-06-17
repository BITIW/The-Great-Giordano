use rand::Rng;
use rand::rng;
use rand::seq::SliceRandom;
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
            (idx + self.size - ((self.shift + self.position) % self.size)) % self.size
        } else {
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
                    'К' => 1, 'Б' => 2, 'Ч' => 3, 'З' => 5, 'Р' => 4,
                    'О' => 6, 'Ф' => 7, 'С' => 8, 'Г' => 9, 'Л' => 10,
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
        let alph_str = if cfg.alphabet == "latin" {
            "abcdefghijklmnopqrstuvwxyz"
        } else {
            "абвгдеёжзийклмнопрстуфхцчшщъыьэюя"
        };
        let alphabet: Vec<char> = alph_str.chars().collect();
        let alphabet_len = alphabet.len();

        let index_map = AlphabetIndex::new(&alphabet);

        let mut plugboard_map = (0..alphabet_len).collect::<Vec<usize>>();
        for &(a, b) in cfg.plugboard.iter() {
            let ia = index_map.get(a).expect("Символ вне алфавита");
            let ib = index_map.get(b).expect("Символ вне алфавита");
            plugboard_map[ia] = ib;
            plugboard_map[ib] = ia;
        }

        let mut blocks: Vec<Block> = cfg
            .blocks
            .iter()
            .map(|s| Block::new(s, alphabet_len))
            .collect();

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
        let lower = msg.to_lowercase();
        let mut input_indices: Vec<Option<usize>> = Vec::with_capacity(lower.len());
        for ch in lower.chars() {
            input_indices.push(self.index_map.get(ch));
        }

        let mut output_indices: Vec<Option<usize>> =
            Vec::with_capacity(input_indices.len());

        for &maybe_idx in input_indices.iter() {
            if let Some(mut idx) = maybe_idx {
                idx = self.plugboard_map[idx];
                for blk in &self.blocks {
                    idx = blk.process_index(idx, false);
                }
                idx = self.reflector.reflect_index(idx);
                for blk in self.blocks.iter().rev() {
                    idx = blk.process_index(idx, true);
                }
                idx = self.plugboard_map[idx];
                for blk in &mut self.blocks {
                    blk.rotate();
                }
                output_indices.push(Some(idx));
            } else {
                output_indices.push(None);
            }
        }

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

/// Для меню: пресет
#[derive(Clone)]
struct Preset {
    name: &'static str,
    description: &'static str,
    blocks: usize,
    speed_idx: u8,
}

const ROTOR_COLORS: &[char] = &['К','Б','Ч','З','Р','О','Ф','С','Г','Л'];

const PRESETS: &[Preset] = &[
    Preset {
        name: "минимально безопасный",
        description: "3 блока, короткие роторы — быстро, но слабее.",
        blocks: 3,
        speed_idx: 8,
    },
    Preset {
        name: "безопасный",
        description: "4 блока, средние роторы — хороший баланс.",
        blocks: 4,
        speed_idx: 7,
    },
    Preset {
        name: "паранойя",
        description: "12 блоков, длинные роторы — медленней, но максимум стойкости.",
        blocks: 12,
        speed_idx: 4,
    },
    Preset {
        name: "Бладислав Ворон",
        description: "О нём мало чего известно, ведь от него получали больше пиздюлей, чем информации, но что известно, так это то что пока одной рукой он делал тихий океан ещё тише, а другой рукой создавал эту бездарную планету и существовать с ним на одной планете это та ещё задача со звёздочкой, награда за которую не предусмотрена",
        blocks: 8_388_608,
        speed_idx: 1,
    },
    Preset {
        name: "Боронислав Владон",
        description: "Пока Бладислав Ворон был занят со своим братом делами галактического масштаба, а мы не знали что делать и чем защищаться, с нами на связь вышел старший двоюрный брат Бладислава и его брата - Боронислав Владон.\nХотите верьте, хотите нет, но пытаясь хоть что либо хоть где либо узнать о Борониславе мы ничего не нашли, даже спрашивая напрямую у Бладислава - данные попросту засекречены всеми возможными грифами секретности, а те кто пытались что-то рассекретить, ну, они получали больше пиздюлей чем информации.\nЗа его работу он потребовал лишь 60 гигиабайт ОЗУ и побольше вычислительных мощностей, ведь его услуги не из дешёвых.",
        blocks: 134_217_728,
        speed_idx: 0,
    },
    Preset {
        name: "Александр \"42\"",
        description: "Уважаемая личность на районе, так именуемый \"42\" в честь количества блоков внутри него.",
        blocks: 42,
        speed_idx: 5,
    },
    Preset {
        name: "Анаколий",
        description: "В любой компании есть самый младший, тут тоже он есть.\n Он самый шустрый и самый малой в компании этих гигантов, но это не мешает ему быть хоть немного грозным, ведь внутри него целых 81.337 бит и хоть 81.337 бит это практически смешно для серьезной защиты, Анаколий предпочитает домашние посиделки за чаем, нежели защиту всего с грифом Top Secret как его старшие братья - а там 81 это вполне достаточно.",
        blocks: 1,
        speed_idx: 10,
    },
];

fn random_blocks<R: Rng>(rng: &mut R, blocks: usize) -> Vec<String> {
    (0..blocks)
        .map(|_| {
            let k = rng.random_range(3..=9);
            (0..k)
                .map(|_| {
                    let idx = rng.random_range(0..ROTOR_COLORS.len());
                    ROTOR_COLORS[idx]
                })
                .collect()
        })
        .collect()
}

fn random_plugboard_pairs<R: Rng>(rng: &mut R, alphabet: &[char]) -> Vec<(char, char)> {
    let mut pool: Vec<char> = alphabet.to_vec();
    pool.shuffle(rng);
    pool.chunks(2)
        .take(8)
        .map(|chunk| (chunk[0], chunk[1]))
        .collect()
}

fn main() {
    // A) Загрузка или генерация конфига
    let cfg = if fs::metadata("esd_config.json").is_ok() {
        print!("Найден конфиг, загрузить? (да/нет): ");
        io::stdout().flush().unwrap();
        if read_line().to_lowercase() == "да" {
            EnigmaSudnogoDnya::load_config("esd_config.json")
                .expect("Не удалось загрузить")
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

    let mut cfg = cfg;

    // B) Генерация нового, если cfg.blocks пустой
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

        let alphabet_str = if cfg.alphabet == "latin" {
            "abcdefghijklmnopqrstuvwxyz"
        } else {
            "абвгдеёжзийклмнопрстуфхцчшщъыьэюя"
        };
        let alphabet_chars: Vec<char> = alphabet_str.chars().collect();

        // 2) Меню пресетов
        println!("\nНастройка конфигурации:");
        println!("0) Я сам всё настрою");
        for (i, p) in PRESETS.iter().enumerate() {
            println!(
                "{:>2}) {} — {} (блоков: {}, скорость: {}/10)",
                i + 1,
                p.name,
                p.description,
                p.blocks,
                p.speed_idx
            );
        }
        print!("Выбор: ");
        io::stdout().flush().unwrap();
        let choice: usize = read_line().parse().unwrap_or(0);

        if choice == 0 {
            // === Ручная настройка (без изменений) ===
            println!("Настройка plugboard (взаимозамен):");
            println!("1) Ввести вручную");
            println!("2) Сгенерировать случайно");
            print!("> ");
            io::stdout().flush().unwrap();
            let pb_choice = read_line();
            let mut plugboard_pairs: Vec<(char, char)> = Vec::new();
            if pb_choice == "1" {
                println!("Вводите пары 'a b'. Пустая строка — выход.");
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
                    plugboard_pairs.push((a, b));
                }
            } else {
                let mut rng = rng();
                plugboard_pairs = random_plugboard_pairs(&mut rng, &alphabet_chars);
                println!("Случайно сгенерированные пары plugboard: {:?}", plugboard_pairs);
            }
            cfg.plugboard = plugboard_pairs;

            print!("Сколько блоков? ");
            io::stdout().flush().unwrap();
            let n: usize = read_line().parse().unwrap_or(4);
            let mut rng = rng();
            cfg.blocks = random_blocks(&mut rng, n);
            cfg.rotor_positions = cfg
                .blocks
                .iter()
                .map(|b| {
                    (0..b.len())
                        .map(|_| rng.random_range(0..alphabet_chars.len()))
                        .collect()
                })
                .collect();

        } else {
            // === Генерация по пресету ===
            let preset = &PRESETS[choice - 1];
            let mut rng = rng();

            cfg.blocks = random_blocks(&mut rng, preset.blocks);
            cfg.rotor_positions = cfg
                .blocks
                .iter()
                .map(|b| {
                    (0..b.len())
                        .map(|_| rng.random_range(0..alphabet_chars.len()))
                        .collect()
                })
                .collect();
            cfg.plugboard = random_plugboard_pairs(&mut rng, &alphabet_chars);

            //println!(
            //    "\nСгенерировано по пресету «{}»:\n  блоки = {:?}\n  пары plugboard = {:?}",
            //    preset.name, cfg.blocks, cfg.plugboard
            //);
        }

        // 3) Сохранить конфиг?
        print!("Сохранить конфиг? (да/нет): ");
        io::stdout().flush().unwrap();
        if read_line().to_lowercase() == "да" {
            serde_json::to_writer_pretty(
                fs::File::create("esd_config.json").unwrap(),
                &cfg,
            )
            .unwrap();
        }
    }

    // C) Основной цикл
    loop {
        print!("Команда (encrypt/decrypt/benchmark/exit): ");
        io::stdout().flush().unwrap();
        match read_line().as_str() {
            "exit" => break,

            "encrypt" => {
                let mut enigma_enc = EnigmaSudnogoDnya::new(&cfg);
                print!("Сообщение: ");
                io::stdout().flush().unwrap();
                let msg = read_line();
                println!("Результат: {}", enigma_enc.encrypt(&msg));
            }

            "decrypt" => {
                let mut enigma_dec = EnigmaSudnogoDnya::new(&cfg);
                print!("Сообщение: ");
                io::stdout().flush().unwrap();
                let msg = read_line();
                println!("Результат: {}", enigma_dec.encrypt(&msg));
            }

            "benchmark" => {
                let mut rng = rng();
                {
                    let alphabet_len = EnigmaSudnogoDnya::new(&cfg).alphabet.len();
                    let total_rotors: usize =
                        cfg.blocks.iter().map(|blk| blk.len()).sum();
                    let plugboard_pairs = cfg.plugboard.len();

                    let log2_positions =
                        (total_rotors as f64) * (alphabet_len as f64).log2();
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
                    let mut text = String::with_capacity(size);
                    let alphabet = EnigmaSudnogoDnya::new(&cfg).alphabet;
                    let a_len = alphabet.len();
                    for _ in 0..size {
                        let idx = rng.random_range(0..a_len);
                        text.push(alphabet[idx]);
                    }

                    let t3 = Instant::now();
                    let mut enc = EnigmaSudnogoDnya::new(&cfg);
                    let cipher = enc.encrypt(&text);
                    let mut dec = EnigmaSudnogoDnya::new(&cfg);
                    let recovered = dec.encrypt(&cipher);
                    let kat_time = t3.elapsed().as_secs_f32();
                    if recovered != text {
                        eprintln!(
                            "KAT FAILED на size = {}: decrypt(encrypt(text)) != text",
                            size
                        );
                    } else {
                        println!("KAT: pass");
                    }

                    let t0 = Instant::now();
                    let mut e1 = EnigmaSudnogoDnya::new(&cfg);
                    let _ = e1.encrypt(&text);
                    let enc_t = t0.elapsed().as_secs_f32();

                    let t1 = Instant::now();
                    let mut e2 = EnigmaSudnogoDnya::new(&cfg);
                    let _ = e2.encrypt(&cipher);
                    let dec_t = t1.elapsed().as_secs_f32();

                    println!(
                        "{} → encrypt: {:.6}, decrypt: {:.6}, KAT: {:.6}",
                        size, enc_t, dec_t, kat_time
                    );
                }
            }

            _ => println!("Неизвестная команда."),
        }
    }
}
