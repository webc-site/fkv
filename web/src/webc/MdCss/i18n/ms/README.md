# Gaya tipografi dioptimumkan untuk pengalaman membaca

- **Penetapan taip asas**: Sesuaikan dengan tajuk, perenggan, tebal, condong, coretan, kod sebaris, gambar dan baris pembahagi.
- **Animasi pautan**: Paparkan animasi garis bawah apabila melayang.
- **Senarai projek**: Senarai tidak tersusun menggunakan titik tumpu, senarai tersusun menggunakan nombor siri latar belakang bulat dan menyokong sarang berbilang peringkat.
- **Senarai Tugas**: Menyediakan gaya kotak semak, menyokong tugasan dan status selesai.
- **Petua Amaran**: Menyokong lima jenis kotak amaran: Nota, Petua, Penting, Amaran dan Perhatian.
- **Jadual dan Kod**: Menyediakan penyerlahan tuding baris jadual dan blok kod dengan bayang-bayang dan latar belakang kecerunan.

## Gunakan demo

```html
<div class="Md">
  <h1>tajuk</h1>
  <p>Teks perenggan dan <a href="#">pautan</a></p>
  <hr>
  <h2>Tajuk peringkat kedua</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Item Tugasan</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Kandungan nota</p>
  </blockquote>
</div>
```

## kelas gaya

Gaya digunakan pada elemen HTML di bawah bekas `.Md`.

### Gaya kotak amaran

Tambahkan nama kelas berikut pada elemen `blockquote` dengan nama kelas `.q` untuk menogol kotak amaran:

- `note`: Nota
- `tip`: Petua
- `important`: Penting
- `warning`: amaran
- `caution`: Nota