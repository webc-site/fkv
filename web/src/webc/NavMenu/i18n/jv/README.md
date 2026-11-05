# Menu navigasi diadaptasi kanggo ukuran layar sing beda

- Desktop: Tautan ditampilake ing kothak horisontal
- Versi seluler: Ambruk menyang tombol menu, klik kanggo geser metu laci sidebar
- Aktivasi rute: mbandhingake path saiki lan nilai hash kanthi otomatis, lan nambah jeneng kelas status aktivasi menyang link sing cocog `A`
- Tutup kanthi otomatis: Nalika terminal seluler ngeklik link utawa latar mburi njaba sidebar, sidebar kanthi otomatis nutup.

## nggunakake

```html
<c-nav-menu>
  <a href="/">kaca ngarep</a>
  <a href="/doc">dokumen</a>
  <a href="/about">babagan</a>
</c-nav-menu>
```

## nggambaraké

Nalika komponen diwiwiti, yen ora ana `c-side` ing unsur anak, tombol menu seluler lan `c-side` sidebar bakal digawe kanthi otomatis, lan kabeh tag `a` bakal dipindhah menyang sidebar.

## kelas gaya

- **A**: Link sing diaktifake bakal otomatis nambah jeneng kelas `A`