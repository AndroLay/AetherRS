//! Data panel emoji: kategori, emoji, dan kata kunci pencarian (Indonesia dan Inggris).
//! Kata kunci berupa teks huruf kecil dipisah spasi. Pencarian mencocokkan substring pada
//! kata kunci dan pada karakter emoji itu sendiri.

/// Satu kategori pada baris tab panel.
pub struct EmoCat {
    pub name: &'static str,
    /// Emoji yang dipakai sebagai ikon tab.
    pub icon: &'static str,
}

/// Satu emoji beserta indeks kategori (ke `EMO_CATS`) dan kata kuncinya.
pub struct EmoItem {
    pub ch: &'static str,
    pub cat: usize,
    pub kw: &'static str,
}

/// Urutan kategori sama dengan urutan tab (kiri ke kanan).
pub const EMO_CATS: [EmoCat; 8] = [
    EmoCat { name: "Wajah", icon: "😀" },
    EmoCat { name: "Orang", icon: "👋" },
    EmoCat { name: "Simbol", icon: "💯" },
    EmoCat { name: "Kerja", icon: "💻" },
    EmoCat { name: "Alam", icon: "🌈" },
    EmoCat { name: "Makanan", icon: "🍕" },
    EmoCat { name: "Aktivitas", icon: "🎉" },
    EmoCat { name: "Perjalanan", icon: "🚀" },
];

pub const EMO_ITEMS: &[EmoItem] = &[
    // Wajah (0)
    EmoItem { ch: "😀", cat: 0, kw: "senyum wajah gembira smile happy grin" },
    EmoItem { ch: "😂", cat: 0, kw: "tertawa lucu ngakak lol laugh joy tears" },
    EmoItem { ch: "🤣", cat: 0, kw: "ngakak guling tertawa rofl laugh" },
    EmoItem { ch: "😊", cat: 0, kw: "senyum malu tersipu blush smile" },
    EmoItem { ch: "😍", cat: 0, kw: "jatuh cinta suka love heart eyes" },
    EmoItem { ch: "😘", cat: 0, kw: "cium kiss cinta sayang" },
    EmoItem { ch: "😎", cat: 0, kw: "keren cool kacamata sunglasses" },
    EmoItem { ch: "🤔", cat: 0, kw: "berpikir mikir hmm think" },
    EmoItem { ch: "😢", cat: 0, kw: "sedih menangis cry sad" },
    EmoItem { ch: "😭", cat: 0, kw: "nangis sangat sedih sob crying" },
    EmoItem { ch: "😡", cat: 0, kw: "marah kesal angry mad" },
    EmoItem { ch: "😴", cat: 0, kw: "tidur ngantuk sleep zzz" },
    EmoItem { ch: "🙄", cat: 0, kw: "malas bosan roll eyes" },
    EmoItem { ch: "😅", cat: 0, kw: "keringat lega sweat relief" },
    EmoItem { ch: "🥺", cat: 0, kw: "memohon mohon puppy eyes please" },
    EmoItem { ch: "🤗", cat: 0, kw: "pelukan peluk hug" },
    EmoItem { ch: "🤩", cat: 0, kw: "takjub kagum wow star eyes" },
    EmoItem { ch: "😉", cat: 0, kw: "kedip wink" },
    EmoItem { ch: "😇", cat: 0, kw: "malaikat suci angel innocent" },
    EmoItem { ch: "🥰", cat: 0, kw: "sayang cinta adore love" },
    EmoItem { ch: "😋", cat: 0, kw: "lezat enak yummy tasty" },
    EmoItem { ch: "🙂", cat: 0, kw: "senyum tipis slight smile" },
    EmoItem { ch: "😬", cat: 0, kw: "canggung meringis awkward grimace" },
    EmoItem { ch: "🥳", cat: 0, kw: "pesta party celebrate" },
    // Orang (1)
    EmoItem { ch: "👍", cat: 1, kw: "setuju oke bagus jempol thumbs up like ok" },
    EmoItem { ch: "👎", cat: 1, kw: "tidak jelek jempol thumbs down dislike" },
    EmoItem { ch: "👏", cat: 1, kw: "tepuk tangan applause clap bravo" },
    EmoItem { ch: "🙏", cat: 1, kw: "tolong mohon terima kasih pray please thanks" },
    EmoItem { ch: "👋", cat: 1, kw: "halo hai dadah hi hello wave bye" },
    EmoItem { ch: "💪", cat: 1, kw: "kuat otot semangat strong muscle" },
    EmoItem { ch: "🙌", cat: 1, kw: "syukur hore yeah celebrate hands" },
    EmoItem { ch: "🤝", cat: 1, kw: "jabat tangan kerja sama deal handshake" },
    EmoItem { ch: "👀", cat: 1, kw: "lihat mata melihat eyes look" },
    EmoItem { ch: "🤷", cat: 1, kw: "tidak tahu bingung shrug" },
    // Simbol (2)
    EmoItem { ch: "❤️", cat: 2, kw: "cinta hati sayang love heart merah" },
    EmoItem { ch: "💔", cat: 2, kw: "patah hati sedih broken heart" },
    EmoItem { ch: "💯", cat: 2, kw: "seratus sempurna hundred perfect" },
    EmoItem { ch: "✨", cat: 2, kw: "kilau bintang sparkle magic baru" },
    EmoItem { ch: "🔥", cat: 2, kw: "api keren hot fire lit" },
    EmoItem { ch: "⭐", cat: 2, kw: "bintang favorit star" },
    EmoItem { ch: "🌟", cat: 2, kw: "bintang berkilau glowing star" },
    EmoItem { ch: "✅", cat: 2, kw: "benar centang selesai check ok done" },
    EmoItem { ch: "❌", cat: 2, kw: "salah silang tidak cross wrong no" },
    EmoItem { ch: "⚠️", cat: 2, kw: "peringatan awas warning caution" },
    // Kerja (3)
    EmoItem { ch: "💡", cat: 3, kw: "ide lampu gagasan idea bulb" },
    EmoItem { ch: "📝", cat: 3, kw: "catatan tulis note memo write" },
    EmoItem { ch: "📌", cat: 3, kw: "pin tandai penting pushpin" },
    EmoItem { ch: "📅", cat: 3, kw: "kalender tanggal jadwal calendar date" },
    EmoItem { ch: "💻", cat: 3, kw: "laptop komputer kerja computer" },
    EmoItem { ch: "📱", cat: 3, kw: "ponsel hp handphone phone mobile" },
    EmoItem { ch: "🔒", cat: 3, kw: "kunci terkunci aman lock secure" },
    EmoItem { ch: "🔑", cat: 3, kw: "kunci password key" },
    EmoItem { ch: "💰", cat: 3, kw: "uang duit kaya money rich" },
    // Alam (4)
    EmoItem { ch: "🌈", cat: 4, kw: "pelangi rainbow" },
    EmoItem { ch: "🐶", cat: 4, kw: "anjing dog puppy" },
    EmoItem { ch: "🐱", cat: 4, kw: "kucing cat kitty" },
    EmoItem { ch: "🌸", cat: 4, kw: "bunga sakura flower cherry" },
    EmoItem { ch: "🌻", cat: 4, kw: "matahari bunga sunflower" },
    EmoItem { ch: "🌙", cat: 4, kw: "bulan malam moon night" },
    EmoItem { ch: "☀️", cat: 4, kw: "matahari cerah sun sunny" },
    EmoItem { ch: "🌊", cat: 4, kw: "ombak laut wave sea" },
    EmoItem { ch: "🐢", cat: 4, kw: "kura-kura penyu turtle" },
    EmoItem { ch: "🦋", cat: 4, kw: "kupu-kupu butterfly" },
    // Makanan (5)
    EmoItem { ch: "☕", cat: 5, kw: "kopi teh coffee cup minuman" },
    EmoItem { ch: "🍕", cat: 5, kw: "pizza makan food" },
    EmoItem { ch: "🍎", cat: 5, kw: "apel buah apple fruit" },
    EmoItem { ch: "🍔", cat: 5, kw: "burger hamburger makan" },
    EmoItem { ch: "🍰", cat: 5, kw: "kue cake dessert ulang tahun" },
    EmoItem { ch: "🍜", cat: 5, kw: "mie ramen noodle makan" },
    EmoItem { ch: "🍵", cat: 5, kw: "teh hijau tea minuman" },
    EmoItem { ch: "🍩", cat: 5, kw: "donat donut manis sweet" },
    // Aktivitas (6)
    EmoItem { ch: "🎉", cat: 6, kw: "pesta selamat party congrats tada" },
    EmoItem { ch: "🎁", cat: 6, kw: "hadiah kado gift present" },
    EmoItem { ch: "🎵", cat: 6, kw: "musik lagu music note" },
    EmoItem { ch: "🎯", cat: 6, kw: "target sasaran bullseye dart" },
    EmoItem { ch: "🎮", cat: 6, kw: "game permainan gaming controller" },
    EmoItem { ch: "⚽", cat: 6, kw: "bola sepak ball soccer football" },
    EmoItem { ch: "🎨", cat: 6, kw: "lukis seni art paint" },
    EmoItem { ch: "📷", cat: 6, kw: "kamera foto camera photo" },
    EmoItem { ch: "🎬", cat: 6, kw: "film video movie clapper" },
    EmoItem { ch: "🏆", cat: 6, kw: "piala juara trophy win" },
    // Perjalanan (7)
    EmoItem { ch: "🚀", cat: 7, kw: "roket luncur launch rocket" },
    EmoItem { ch: "🌍", cat: 7, kw: "dunia bumi globe world earth" },
    EmoItem { ch: "✈️", cat: 7, kw: "pesawat terbang plane flight" },
    EmoItem { ch: "🚗", cat: 7, kw: "mobil car drive" },
    EmoItem { ch: "🏠", cat: 7, kw: "rumah home house" },
    EmoItem { ch: "🚲", cat: 7, kw: "sepeda bike bicycle" },
    EmoItem { ch: "🗺️", cat: 7, kw: "peta map" },
    EmoItem { ch: "⛰️", cat: 7, kw: "gunung mountain" },
    EmoItem { ch: "🏖️", cat: 7, kw: "pantai liburan beach vacation" },
    EmoItem { ch: "🚉", cat: 7, kw: "stasiun kereta station train" },
];
