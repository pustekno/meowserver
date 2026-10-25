// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 PT Pusat Teknologi Nusantara and PT Puskomedia Indonesia Kreatif
// Additional terms under GPLv3 section 7(b) apply; see NOTICE.

//! Translations for the few texts built in Rust (status lines, messages).
//! Everything in the `.slint` files uses `@tr` and the `.po` files instead.

use std::sync::atomic::{AtomicBool, Ordering};

static INDONESIAN: AtomicBool = AtomicBool::new(false);

pub fn set_language(code: &str) {
    INDONESIAN.store(code == "id", Ordering::Relaxed);
}

/// `text` in the current language. Unknown texts stay in English.
pub fn t(text: &str) -> String {
    if !INDONESIAN.load(Ordering::Relaxed) {
        return text.to_owned();
    }
    let id = match text {
        "MeowServer is not running" => "MeowServer tidak berjalan",
        "A service failed — see Home" => "Ada layanan yang gagal — lihat Beranda",
        "Stopped" => "Berhenti",
        "Running" => "Berjalan",
        "running" => "berjalan",
        "stopped" => "berhenti",
        "processes" => "proses",
        "Memory limit" => "Batas memori",
        "Largest upload" => "Ukuran unggahan terbesar",
        "Largest form post" => "Ukuran kiriman form terbesar",
        "Time limit per request (seconds)" => "Batas waktu per request (detik)",
        "Show errors (On/Off)" => "Tampilkan error (On/Off)",
        "Time zone" => "Zona waktu",
        "PHP settings saved and applied." => "Pengaturan PHP disimpan dan diterapkan.",
        "Ready. Put a project in www/ or use Sites → Add project." => {
            "Siap. Taruh proyek di www/ atau pakai Situs → Tambah proyek."
        }
        "is no longer served. No files were touched." => "tidak lagi dilayani. Tidak ada file yang diubah.",
        "Added" => "Ditambahkan",
        "Created" => "Dibuat",
        "Saved" => "Disimpan",
        "Stopping all services…" => "Menghentikan semua layanan…",
        "looking for versions…" => "mencari versi…",
        "of" => "dari",
        "Starting…" => "Menjalankan…",
        "Copying databases" => "Menyalin database",
        "Looking at what can be imported…" => "Memeriksa apa yang bisa diimpor…",
        "{} was not found." => "{} tidak ditemukan.",
        "Import from {} ({}):" => "Impor dari {} ({}):",
        "serve every folder in {}" => "layani setiap folder di {}",
        "copy the {} databases ({})" => "salin database {} ({})",
        "copy the {} databases ({}); MeowServer's empty one is set aside" => {
            "salin database {} ({}); database MeowServer yang masih kosong disisihkan"
        }
        "skip the {} databases: MeowServer's already has databases ({})" => {
            "lewati database {}: milik MeowServer sudah berisi database ({})"
        }
        "Project folders stay where they are. Nothing in {} is changed." => {
            "Folder proyek tetap di tempatnya. Tidak ada yang diubah di {}."
        }
        "{}'s database server is still running (closing {} does not always stop it). \
         Stop it so its databases can be copied safely." => {
            "Server database {} masih berjalan (menutup {} tidak selalu menghentikannya). \
             Hentikan dulu agar database-nya bisa disalin dengan aman."
        }
        "Stopping {}'s database…" => "Menghentikan database {}…",
        "Imported." => "Selesai diimpor.",
        "Started. The first start of a project can take a while; see its Log." => {
            "Dijalankan. Start pertama sebuah proyek bisa agak lama; lihat Log-nya."
        }
        "copy {} ({}) to {}, as {}" => "salin {} ({}) ke {}, sebagai {}",
        "serve {} where it is, as {}" => "layani {} di tempatnya, sebagai {}",
        "make PHP {} the default" => "jadikan PHP {} sebagai default",
        "install PHP {} and make it the default" => "pasang PHP {} dan jadikan default",
        "skip {}: the link's target does not exist" => "lewati {}: tujuan link-nya tidak ada",
        "Projects stored in {} are copied, linked ones are served where they are, and nothing in {} is changed, so it can be removed afterwards." => {
            "Proyek yang tersimpan di {} disalin, proyek berupa link dilayani di tempatnya, dan tidak ada yang diubah di {}, jadi setelah ini aplikasi itu boleh dihapus."
        }
        "Copying projects and databases" => "Menyalin proyek dan database",
        "Needs PHP {}, but would run on PHP {}." => "Butuh PHP {}, tapi akan jalan di PHP {}.",
        "Install PHP {}" => "Pasang PHP {}",
        "Checking for updates…" => "Memeriksa update…",
        "MeowServer {} is available" => "MeowServer {} tersedia",
        "MeowServer is up to date." => "MeowServer sudah versi terbaru.",
        "Downloading the update:" => "Mengunduh update:",
        "Downloading {}…" => "Mengunduh {}…",
        "{} {} installed for PHP {} and turned on." => "{} {} terpasang untuk PHP {} dan sudah diaktifkan.",
        "Update PHP {}" => "Perbarui PHP {}",
        "Connects to {} on port {}, but MeowServer's runs on {}." => {
            "Terhubung ke {} di port {}, tapi milik MeowServer jalan di {}."
        }
        "Use port {} for {}" => "Pakai port {} untuk {}",
        "Stopping the database that holds port {}…" => "Menghentikan database yang memakai port {}…",
        "{} now uses port {}." => "{} sekarang memakai port {}.",
        "Port {} is used by {}. Close that program first." => "Port {} dipakai {}. Tutup program itu dulu.",
        "Port {} is used by MeowServer's {}." => "Port {} dipakai {} milik MeowServer.",
        "Fix: {}" => "Solusi: {}",
        "Runs on {}, which is not installed." => "Berjalan dengan {}, yang belum terpasang.",
        "Needs {} {}; no installed version fits." => "Butuh {} {}; belum ada versi terpasang yang cocok.",
        "Install {} {}" => "Pasang {} {}",
        "Needs PHP {}; no PHP is installed." => "Butuh PHP {}; belum ada PHP yang terpasang.",
        "Uses {}, which is not installed." => "Memakai {}, yang belum terpasang.",
        "Install {}" => "Pasang {}",
        "{} {} runs on port {}." => "{} {} berjalan di port {}.",
        "{} {} stopped. Its data is kept." => "{} {} dihentikan. Datanya tetap disimpan.",
        "Trusted. Restart the browser if it was open." => {
            "Dipercaya. Mulai ulang browser jika sedang terbuka."
        }
        "installed." => "terpasang.",
        "is now active." => "sekarang aktif.",
        "Removed" => "Dihapus",
        "Folders excluded from Defender scanning." => "Folder dikecualikan dari pemindaian Defender.",
        "Not changed: administrator rights were declined." => "Tidak diubah: hak admin ditolak.",
        other => other,
    };
    id.to_owned()
}
