# Dokumentasi dan kod sumber untuk memaparkan komponen atau fungsi alat

- Reka letak responsif dua lajur, dengan dokumen dipaparkan di sebelah kiri dan kod sumber dipaparkan di sebelah kanan
- Dokumentasi dan sokongan kod sumber Edit dan butang pintasan Kod untuk melompat ke alamat sumber.
- Tajuk dan pengenalan dipaparkan di bahagian atas, menyokong terjemahan automatik i18n.

## Gunakan demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parameter harta

- `src`: Alamat URL kod sumber.
- `href`: Alamat URL penurunan nilai dokumen, menyokong penggunaan `{code}` pemegang tempat untuk menyesuaikan diri dengan bahasa semasa.