# pituduh gelembung ngambang

Tampilake informasi tooltip hover nalika mouse diaktifake utawa unsur diaktifake.

- Adhedhasar Komponen Web asli, entheng lan kinerja dhuwur
- Ndhukung posisi spasial adaptif ing patang arah (kelas = "ndhuwur / ngisor / kiwa / tengen")
- Iki nggunakake latar mburi gradien tekstur kaca beku ireng lan karakter putih, kanthi sorotan kontur pinggiran fisik sing apik banget.
- Pitungan adaptif posisi sing dibangun, kanthi otomatis nyetel arah pop-up miturut papan sing isih ana ing viewport
- Gunakake `slot="pop"` kanggo nampilake konten pituduh ngambang

## Gunakake demo

```html
<!-- Etung arah kanthi otomatis (disaranake, ora perlu ngumumake kelas) -->
<c-pophover>
  <button>otomatis</button>
  <div slot="pop">Iki minangka tip gelembung</div>
</c-pophover>

<!-- arah tetep -->
<c-pophover class="left">
  <button>Ngiwa</button>
  <div slot="pop">Iki minangka tip gelembung</div>
</c-pophover>

<c-pophover class="right">
  <button>bener</button>
  <div slot="pop">Iki minangka tip gelembung</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Mudhun</button>
  <div slot="pop">Iki minangka tip gelembung</div>
</c-pophover>

<c-pophover class="top">
  <button>unggul</button>
  <div slot="pop">Iki minangka tip gelembung</div>
</c-pophover>
```

## variabel CSS

Gaya khusus didhukung liwat variabel CSS ing ngisor iki:

| jeneng variabel | nilai standar | nggambaraké |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Spasi ing antarane tooltip gelembung lan unsur pemicu |
| `--pophover-margin` | `24px` | Margin safety pinggiran viewport sajrone pitungan posisi adaptif |
| `--pophover-bg-top` | `#18181ce0` | Warna ndhuwur latar mburi gradien gelembung |
| `--pophover-bg-mid` | `#121215e5` | Werna latar mburi panah ing sisih kiwa lan tengen gelembung |
| `--pophover-bg-bottom` | `#0c0c0feb` | Werna panah sisih ngisor lan ndhuwur lan ngisor latar mburi gradien gelembung |

## Gaya tooltip ikon (`.Ico`)

Kanggo tombol pituduh ngambang jinis lambang murni (kayata "Sunting", "Busak", lsp.), sampeyan bisa ngetrapake lambang gabungan lan gaya hover kanthi nambahake jeneng kelas `.Ico` menyang wadhah njaba utawa komponen kasebut dhewe.

### Tuladha struktur

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">nyunting</div>
  </c-pophover>
</b>
```

Tata letak dhasar, ukuran (18x18px), lan owah-owahan werna hover wis digabungake ing `.Ico > c-pophover > a`. Pangembang mung kudu nemtokake file lambang sing cocog kanthi kapisah, kayata:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```