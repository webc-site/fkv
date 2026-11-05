# upau wa kusogeza wa mtandaoni

- **Usogezaji wa njia mbili**: Hutoa usogezaji wa wima (`c-vs`) na mlalo (`c-hs`).
- **Jibu la ukubwa**: Fuatilia mabadiliko ya maudhui na ukubwa wa eneo, sasisha kiotomatiki ukubwa na nafasi ya upau wa kusogeza, na ufiche kiotomatiki wakati hakuna vifurushi.
- **Usaidizi mwingiliano**: Inaauni utembezaji wa gurudumu, uvutaji wa kitelezi na uwekaji nafasi wa kubofya.
- **Ficha Hali**: Onyesha na upanue kitelezi wakati unaelea au unatembeza, na uifiche kiotomatiki ikiwa imesimama.

## kusongesha wima

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## kitabu cha mlalo

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## sehemu za mtindo

Geuza kukufaa mtindo wa vipengele vya ndani vya Kivuli DOM kupitia `::part()`:

- `::part(scroll)`: Sogeza eneo la maudhui.
- `::part(bar)`: Wimbo wa upau wa kusogeza.
- `::part(si)`: Kitelezi cha upau wa kusogeza.
- `::part(i)`: Safu ya kuifunga ya ndani.
- `::part(drag)`: Wimbo wa upau wa kusogeza katika hali ya kuburuta.

## darasa la mtindo

- `.drag`: Inatumika kwa kipengele cha `body`, ikionyesha kuwa kiko katika hali ya kuburuta.

## Tabia maalum

- `--si-bg`: Rangi ya mandharinyuma ya kitelezi.
- `--si-anim`: Uhuishaji wa kitelezi.
- `--cursorScrollhSvg`: Kishale cha kipanya wakati upau wa kusogeza ulio mlalo unapoelea.
- `--cursorScrollvSvg`: Kishale cha kipanya wakati upau wa kusogeza wima unaelea.