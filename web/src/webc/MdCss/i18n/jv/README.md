# Gaya tipografi dioptimalake kanggo pengalaman maca

- **Setelan dhasar**: Adaptasi kanggo judhul, paragraf, kandel, miring, coretan, kode inline, gambar lan garis pamisah.
- **Animasi link**: Tampilake animasi garis ngisor nalika nglayang.
- **Dhaptar proyek**: Dhaptar sing ora diurutake nggunakake peluru, dhaptar sing diurutake nggunakake nomer seri latar mburi sing dibunderaké, lan ndhukung nesting multi-level.
- **Dhaptar Tugas**: Nyedhiyani gaya kothak centhang, ndhukung tugas lan status rampung.
- **Tips Pènget**: Ndhukung limang jinis kothak bebaya: Cathetan, Tip, Penting, Pènget, lan Perhatian.
- **Tabel lan Kode**: Nyedhiyakake sorotan hover baris tabel, lan blok kode kanthi bayangan lan latar mburi gradien.

## Gunakake demo

```html
<div class="Md">
  <h1>judhul</h1>
  <p>Teks paragraf lan <a href="#">link</a></p>
  <hr>
  <h2>Judhul tingkat kapindho</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Item Tugas</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Konten cathetan</p>
  </blockquote>
</div>
```

## kelas gaya

Gaya ditrapake kanggo unsur HTML ing wadhah `.Md`.

### Gaya kothak tandha

Tambah jeneng kelas ing ngisor iki ing `blockquote` unsur karo `.q` jeneng kelas kanggo ngalih kothak tandha:

- `note`: Cathetan
- `tip`: Tip
- `important`: Penting
- `warning`: warning
- `caution`: Cathetan