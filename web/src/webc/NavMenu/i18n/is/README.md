# Leiðsöguvalmynd aðlagaður mismunandi skjástærðum

- Skrifborð: Tenglar eru sýndir í láréttum flísum
- Farsímaútgáfa: Dragðu saman í valmyndarhnapp, smelltu til að renna út skúffu hliðarstikunnar
- Leiðarvirkjun: berðu sjálfkrafa saman núverandi slóð og kjötkássagildi og bættu heiti virkjunarstöðuflokks við samsvarandi tengil `A`
- Loka sjálfkrafa: Þegar farsímaútstöðin smellir á tengil eða ytri bakgrunn hliðarstikunnar lokar hliðarstikan sjálfkrafa.

## nota

```html
<c-nav-menu>
  <a href="/">forsíðu</a>
  <a href="/doc">skjal</a>
  <a href="/about">um</a>
</c-nav-menu>
```

## sýna

Þegar íhluturinn er frumstilltur, ef ekkert `c-side` er í barnaeiningunni, verða farsímavalmyndarhnappurinn og `c-side` hliðarstikan sjálfkrafa búin til og öll `a` merki verða færð inn í hliðarstikuna.

## stílaflokkur

- **A**: Virkjaði hlekkurinn bætir sjálfkrafa við bekkjarheitinu `A`