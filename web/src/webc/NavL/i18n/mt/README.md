# Awtomatikament taqleb il-menu tan-navigazzjoni u l-buttuna tar-ritorn

- Kebbeb awtomatikament elementi tfal biex tiġġenera menu tan-navigazzjoni u strutturi tal-buttuni lura
- Intraċċa awtomatikament l-istorja tar-rotta u kkalkula l-mogħdija tar-ritorn preċedenti
- Uri l-menu tan-navigazzjoni meta r-rotta taqbel mal-oġġett tal-menu, inkella uri l-buttuna lura

## Uża d-demo

```html
<c-nav-l>
  <a href="/">paġna ta’ quddiem</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## klassi tal-istil

- `B`: jattiva l-istat tal-buttuna tar-ritorn. Uri l-buttuna lura u aħbi l-menu tan-navigazzjoni. Mibdula awtomatikament fi ħdan il-komponent ibbażat fuq ir-rotta.