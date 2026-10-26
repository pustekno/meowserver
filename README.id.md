# MeowServer

Lingkungan pengembangan web lokal yang ringan untuk Windows, macOS, dan
Linux. Taruh proyek di `www/` (atau tambahkan folder mana pun), lalu buka di
`proyek.localhost`.

Tanpa iklan. Tanpa telemetri. Tanpa akun.

[English](README.md)

> **Status: pra-rilis.** Semua fitur di bawah sudah jalan dan diuji di
> Windows 11. Build macOS dan Linux sudah bisa dikompilasi tapi belum
> dijalankan. Lihat [`docs/PRD.md`](docs/PRD.md) §16.

## Unduh

Ambil `MeowServer-<versi>-windows-x86_64-setup.exe` (atau `.zip` portable)
dari [halaman Releases](https://github.com/pustekno/meowserver/releases).
Versi sebelum 1.0 ditandai sebagai pre-release.

MeowServer belum ditandatangani (code signing), jadi Windows mungkin
menampilkan "Windows protected your PC": pilih **More info → Run anyway**.
Beberapa langkah yang mengubah Windows sendiri (entri hosts untuk `.test`,
memercayai sertifikat HTTPS, pengecualian Defender) meminta hak
administrator sekali dan menampilkan "Penerbit tidak diketahui" karena
alasan yang sama.

## Mulai cepat (Windows)

1. Jalankan setup. Terpasang untuk user kamu saja, tanpa hak admin, ke
   `C:\MeowServer` atau folder pilihanmu. Setup menanyakan web server
   (Caddy, atau Nginx/Apache bila lebih suka) dan database (MySQL 8.4
   seperti Laragon, atau MariaDB/PostgreSQL), lalu mengunduhnya bersama PHP
   dan Composer dari sumber resminya.
2. Buka MeowServer; semuanya sudah siap dijalankan. Jika Laragon, XAMPP,
   WampServer, atau FlyEnv ditemukan, Beranda menampilkan apa yang akan
   diimpor, dan tidak ada yang disalin sebelum kamu menekan Impor.
3. Taruh proyek di `www\`, atau pakai **Situs → Tambah proyek**, lalu buka
   `http://<nama>.localhost`.

Versi zip portable, atau setup yang dijalankan saat offline, menawarkan
**Pasang paket yang disarankan** di Beranda.

## Fitur

**Web dan PHP**
- Caddy (default), Nginx, atau Apache. Memasang web server lain di halaman
  Versi langsung memakainya; kembali lewat Pengaturan.
- Beberapa versi PHP berdampingan, dipilih per proyek atau otomatis dari
  `composer.json`. Ekstensi PECL (imagick, redis, apcu, …) sekali klik,
  dan Xdebug.
- Situs dikompilasi ke OPcache di latar belakang begitu ditambahkan, jadi
  kunjungan pertama pun cepat.
- Setiap folder di `www/` jadi situs dalam sedetik, termasuk symlink dan
  junction. Proyek di tempat lain (`D:\projects\app`) ditambahkan dengan
  satu klik. Tiap proyek memakai `.localhost` (default) atau `.test`.
- HTTPS lokal dari CA yang hanya berlaku untuk `.localhost`, `.test`, dan
  komputer ini.

**Database**
- MySQL, MariaDB, dan PostgreSQL, dengan beberapa versi MySQL atau
  MariaDB berjalan bersamaan, masing-masing di port sendiri.
- Buka di **phpMyAdmin** atau **Adminer** (Adminer juga untuk PostgreSQL)
  dari halaman Database atau tombol **Admin** di Beranda, langsung masuk.
  HeidiSQL ditawarkan kalau terpasang.
- Backup, restore, dan upgrade versi dengan backup otomatis lebih dulu.

**Layanan dan bahasa lain**
- Redis dan Mailpit (menangkap email yang dikirim proyekmu).
- Proyek Node.js, Python, dan Go: terdeteksi, diberi versi yang cocok, dan
  dev server-nya bisa diakses lewat nama proyek.
- Menyalakan layanan yang belum terpasang di Beranda akan mengunduhnya
  dulu.

**Pindah dari aplikasi lain**
- Mengimpor proyek dan database dari Laragon, XAMPP, WampServer, atau
  FlyEnv yang terdeteksi otomatis. Proyek yang tersimpan di dalam
  aplikasi lama ikut disalin, jadi aplikasi itu bisa dihapus setelahnya.
- Kebutuhan tiap proyek (versi PHP, database, versi Node) dibaca dari
  file-nya; Beranda menyebutkan yang kurang, lengkap dengan tombol pasang.
- Kalau port dipakai program lain (MySQL Laragon di 3306), MeowServer
  pindah ke port kosong dan kembali begitu port itu bebas.

**Kecil dan menjaga privasi**
- Unduhan hanya dari sumber resmi, dengan checksum diverifikasi.
- Ikon tray (±14 MB RAM) dan jendela native yang kecil. Tanpa Electron,
  tanpa webview. Program bantu berjalan tanpa jendela CMD.

## Database

Halaman Database menampilkan host, port, dan user tiap server (`root` untuk
MySQL dan MariaDB, `postgres` untuk PostgreSQL, tanpa password; hanya bisa
diakses dari komputer ini). **Buka di:** membukanya di browser lewat
`http://phpmyadmin.localhost` atau `http://adminer.localhost`; klik pertama
mengunduh alatnya.

## Memperbarui

Di MeowServer: **Pengaturan → Periksa update → Update sekarang**. Atau
jalankan sendiri setup yang lebih baru. Keduanya hanya mengganti file
program; proyek (`www`), database (`data`), pengaturan (`meow.toml`), dan
versi yang sudah diunduh (`bin`) tetap ada. Folder portable diperbarui
dengan cara yang sama: pilih folder itu sebagai folder instalasi.
Uninstall juga tidak menghapusnya; hapus foldernya sendiri kalau sudah
tidak dipakai.

## Lewat terminal

```
meow up                            # jalankan layanan
meow link D:\projects\my-app       # → http://my-app.localhost
meow site my-app --domain test     # → http://my-app.test
meow new laravel blog              # → http://blog.localhost
meow install php 8.3               # versi PHP lain
meow site blog --php 8.3           # pakai untuk satu proyek
meow ext install imagick           # ekstensi PECL
meow dev start my-next-app         # dev server Node, Python, atau Go
meow ssl trust                     # https:// tanpa peringatan
meow doctor                        # apa yang salah, dan cara memperbaikinya
```

`meow --help` menampilkan semua perintah.

## Folder

```
MeowServer/
├── meowd(.exe)        tray dan pengelola layanan
├── meowserver(.exe)   jendela
├── meow(.exe)         command line
├── meow.toml          pengaturan, termasuk per situs
├── bin/               layanan yang diunduh, satu folder per versi
├── etc/generated/     konfigurasi yang ditulis MeowServer (jangan diubah)
├── etc/custom/        penyesuaianmu, tidak pernah disentuh
├── data/              file database, per layanan dan versi
├── www/               proyekmu
├── logs/
└── ssl/               certificate authority lokal
```

## Privasi

MeowServer tidak mengirim apa pun tentang kamu atau proyekmu: tanpa
telemetri, tanpa penghitungan instalasi, tanpa iklan. Permintaan yang
dibuatnya sendiri hanyalah pemeriksaan update harian ke GitHub (opsional,
mati secara default) dan unduhan yang kamu minta. Setelah dua minggu
dipakai, Beranda sekali bertanya apakah kamu mau membagikan MeowServer
atau memberi bintang di GitHub; "Jangan tampilkan lagi" menghentikannya.

## Komunitas

- Pertanyaan, ide, dan berbagi setup:
  [Discussions](https://github.com/pustekno/meowserver/discussions)
- Ada yang rusak: **Pengaturan → Laporkan masalah**, atau
  [buat issue](https://github.com/pustekno/meowserver/issues/new/choose)
- Ikut berkontribusi: lihat [CONTRIBUTING.md](CONTRIBUTING.md)

Bahasa Indonesia dan Inggris sama-sama boleh.

## Build dari source

```
cargo build --release
cargo xtask dist        # zip portable dan setup di dist/
```

Lihat [CONTRIBUTING.md](CONTRIBUTING.md) (bahasa Inggris).

## Lisensi

GPL-3.0-or-later dengan syarat tambahan atribusi berdasarkan Pasal 7(b).
Lihat [LICENSE](LICENSE) dan [NOTICE](NOTICE). Nama dan logo MeowServer
diatur di [TRADEMARK.md](TRADEMARK.md).

Dibuat awalnya oleh **Pustekno** (https://pustekno.id) dan **Puskomedia**
(https://www.puskomedia.id).
