# PDFKit

Semua alat PDF dalam satu aplikasi desktop yang ringan dan **100% offline**, terinspirasi dari situs alat PDF online seperti iLovePDF. Bedanya, file kamu tidak pernah diunggah ke mana pun: semua proses berjalan di komputermu sendiri.

Dibangun dengan [Tauri 2](https://tauri.app) (Rust + WebView2 bawaan Windows), jadi installer-nya kecil dan aplikasinya cepat.

## Fitur

| Alat | Keterangan |
|---|---|
| Gabung PDF | Satukan beberapa PDF; urutannya bisa diatur, tiap file dapat bookmark sendiri |
| Pisah PDF | Per rentang (`1-3, 4-6`), setiap N halaman, atau setiap halaman jadi file terpisah |
| Kompres PDF | Tiga tingkat; gambar dikompres ulang dan diperkecil, objek tak terpakai dibuang. Hasil tidak pernah lebih besar dari aslinya |
| Putar PDF | Semua halaman atau halaman tertentu |
| Hapus / Ambil / Urutkan halaman | Pakai daftar halaman seperti `1,3,5-8`, `5-` (sampai akhir), atau `8-5` (terbalik) |
| PDF ke JPG/PNG | 72, 150, atau 300 dpi |
| JPG ke PDF | JPG, PNG, WebP, BMP, GIF, TIFF; ukuran halaman sesuai gambar, A4, atau Letter. JPEG dimasukkan apa adanya tanpa turun kualitas |
| Watermark | Teks, ukuran, transparansi, kemiringan, warna |
| Nomor halaman | Enam posisi dan beberapa format (`1`, `1 / 10`, `Halaman 1 dari 10`) |
| Kunci PDF | Enkripsi AES-256 |
| Buka kunci PDF | Hapus password dari PDF yang password-nya kamu ketahui |
| Perbaiki PDF | Menulis ulang PDF yang rusak dari bagian yang masih bisa dipulihkan |

Hasil selalu disimpan sebagai file baru di folder yang sama dengan file asli (misalnya `laporan_kompres.pdf`). File asli tidak pernah diubah atau ditimpa.

## Offline

- Semua library PDF ikut dikompilasi ke dalam aplikasi; tidak ada yang diunduh saat aplikasi berjalan.
- UI tidak memuat apa pun dari internet (tanpa CDN, tanpa font online), dan CSP-nya hanya mengizinkan file lokal.
- Tidak ada akun, telemetri, maupun auto-update.
- Windows 10/11 sudah punya WebView2. Untuk komputer tanpa WebView2, ubah `bundle.windows.webviewInstallMode` di `src-tauri/tauri.conf.json` menjadi `offlineInstaller` agar installer membawanya sendiri (ukurannya jadi sekitar 130 MB lebih besar).

## Struktur

```
crates/pdfkit-core/   Engine PDF (Rust murni): semua alat + tes end-to-end
src-tauri/            Aplikasi desktop Tauri yang memanggil engine
ui/                   Tampilan: HTML, CSS, JS polos tanpa framework
```

Library utama: [`lopdf`](https://crates.io/crates/lopdf) untuk membaca/menulis PDF, [`hayro`](https://crates.io/crates/hayro) untuk merender halaman ke gambar, dan [`image`](https://crates.io/crates/image) untuk gambar. Semuanya Rust murni, tanpa DLL tambahan.

## Menjalankan dari kode

Butuh [Rust](https://rustup.rs), [Node.js](https://nodejs.org), dan di Windows: Visual Studio Build Tools (workload "Desktop development with C++").

```bash
npm install
npm run dev          # jalankan aplikasi dalam mode pengembangan
npm run build        # buat installer di target/release/bundle/
cargo test -p pdfkit-core   # tes semua alat
```

## Batasan saat ini

- Watermark dan nomor halaman memakai font Helvetica bawaan PDF, jadi hanya huruf Latin; karakter lain jadi `?`.
- Belum ada OCR dan konversi Word/Excel/PowerPoint ↔ PDF.
- PDF ke gambar memakai renderer `hayro` yang masih berkembang; PDF yang sangat kompleks bisa sedikit berbeda tampilannya.

## Lisensi

MIT
