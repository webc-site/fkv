# Ukurasa wa kichupo ulio na chaguo kunjuzi

- Inaauni kupachika kisanduku cha uteuzi kunjuzi cha upana unaobadilika (`<c-select>`) katika lebo, ambayo itaonyeshwa tu ukurasa wa kichupo cha sasa utakapowashwa.
- Ubainishaji wa muundo: Kipengele cha mtoto hutumia `nav > a` kama uelekezaji wa lebo, na kipengele cha mtoto cha moja kwa moja `b` hutumika kama eneo la maudhui.
- Huhusisha kiotomatiki `a[value]` tagi, `a` na `c-select` (kulingana na sifa ya thamani ya kikundi na chaguo iliyochaguliwa) na `b[slot]` nodi ya maudhui.
- Inaauni kuendelea kiotomatiki kipengee kilichoamilishwa kwa sasa hadi `localStorage` kupitia `key`sifa.
- Inaauni uhifadhi otomatiki na urejeshaji wa teuzi ndogo za kihistoria kwa kila kikundi ili `localStorage` kwa kutumia `key + '/' + tab`.
- Inaauni kubainisha kichupo amilifu kupitia `value` sifa.

## Tumia onyesho

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>lugha</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>mfumo</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript ni lugha ya maandishi</b>
    <b slot="rs">Rust ni lugha ya programu ya kiwango cha mifumo</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS ni mfumo wa uendeshaji wa picha wa msingi wa Unix</b>
    <b slot="win">Windows ni mfumo wa uendeshaji uliotengenezwa na Microsoft</b>
  </b>
</c-tab-select>
```

## mali

- `key`: jina kuu linalotumika kwa uendelevu wa Hifadhi ya ndani
- `value`: thamani iliyochaguliwa kwa sasa

## Matukio maalum

- `change`: Imeanzishwa wakati wa kubadilisha vichupo, `e.value` ndio `value` thamani ya chaguo lililoamilishwa kwa sasa.