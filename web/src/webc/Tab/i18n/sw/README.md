# Vichupo vinavyoweza kubadilishwa

- Hali iliyochaguliwa ya lebo imeunganishwa kwa urahisi na eneo la maudhui hapa chini.
- Vipimo vya muundo: Vipengele vya mtoto vimerekebishwa ili kutumia `nav > a` kama uelekezaji wa lebo, na kipengele cha mtoto cha moja kwa moja `b` kinatumika kama eneo la maudhui.
- Unganisha kiotomatiki lebo ya `a[value]` na `b[slot]` nodi ya maudhui.
- Inaauni kuendeleza kiotomatiki ukurasa unaotumika hadi `localStorage` kupitia `key` sifa.
- Inaauni kubainisha ukurasa wa kichupo ulioamilishwa kupitia `value` sifa (ikiwa hakuna akiba na `value`, lebo iliyo na `class="A"` au tagi ya kwanza itachaguliwa kwa chaguomsingi).
- Utekelezaji wa DOM ya Mwanga mwepesi, hakuna kizuizi cha DOM ya Kivuli, uhuru wa hali ya juu katika ubinafsishaji wa mtindo.

## Tumia onyesho

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">tufaha</a>
    <a value="banana">ndizi</a>
    <a value="orange">tangerine</a>
  </nav>
  <b slot="apple">Apple ni tunda lenye lishe</b>
  <b slot="banana">Ndizi ni tunda la kitropiki</b>
  <b slot="orange">Machungwa yana vitamini C kwa wingi</b>
</c-tab>
```

## mali

- `key`: jina kuu linalotumika kwa uendelevu wa Hifadhi ya ndani
- `value`: thamani iliyochaguliwa kwa sasa

## Matukio maalum

- `change`: Huanzishwa wakati kichupo kinabadilishwa, `e.value` ndio `value` thamani ya lebo iliyoamilishwa kwa sasa.