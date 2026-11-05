# haraka ya Bubble inayoelea

Onyesha maelezo ya kidokezo cha kielelezo wakati kipanya kinapoelea au kipengele kimewashwa.

- Kulingana na Kipengele asili cha Wavuti, uzani mwepesi na utendakazi wa hali ya juu
- Inaauni nafasi ya anga inayobadilika katika pande nne (darasa = "juu/chini/kushoto/kulia")
- Inatumia mandharinyuma ya glasi nyeusi iliyoganda na herufi nyeupe, yenye vivutio vya kupendeza vya ukingo.
- Hesabu inayoweza kubadilika ya nafasi iliyojengewa ndani, hurekebisha kiotomati mwelekeo wa ibukizi kulingana na nafasi iliyobaki kwenye lango la kutazama.
- Tumia `slot="pop"` kuonyesha maudhui ya haraka yanayoelea

## Tumia onyesho

```html
<!-- Hesabu mwelekeo kiotomatiki (inapendekezwa, hakuna haja ya kutangaza darasa) -->
<c-pophover>
  <button>moja kwa moja</button>
  <div slot="pop">Hiki ni kidokezo cha Bubble</div>
</c-pophover>

<!-- mwelekeo uliowekwa -->
<c-pophover class="left">
  <button>Kushoto</button>
  <div slot="pop">Hiki ni kidokezo cha Bubble</div>
</c-pophover>

<c-pophover class="right">
  <button>kulia</button>
  <div slot="pop">Hiki ni kidokezo cha Bubble</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Chini</button>
  <div slot="pop">Hiki ni kidokezo cha Bubble</div>
</c-pophover>

<c-pophover class="top">
  <button>mkuu</button>
  <div slot="pop">Hiki ni kidokezo cha Bubble</div>
</c-pophover>
```

## Vigezo vya CSS

Mitindo maalum inasaidiwa kupitia anuwai zifuatazo za CSS:

| jina la kutofautiana | thamani chaguo-msingi | onyesha |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Nafasi kati ya kidirisha cha viputo na kipengele cha kufyatua |
| `--pophover-margin` | `24px` | Ukingo wa ukingo wa usalama wa kituo cha kutazama wakati wa hesabu ya nafasi inayobadilika |
| `--pophover-bg-top` | `#18181ce0` | Rangi ya juu ya mandharinyuma ya mwinuko wa kiputo |
| `--pophover-bg-mid` | `#121215e5` | Rangi ya usuli ya mishale kwenye pande za kushoto na kulia za kiputo |
| `--pophover-bg-bottom` | `#0c0c0feb` | Rangi ya vishale vya upande wa chini na wa juu na chini wa mandharinyuma ya kiputo |

## Mtindo wa kidokezo cha ikoni (`.Ico`)

Kwa vitufe vya kuuliza vinavyoelea vya aina ya ikoni (kama vile "Hariri", "Futa", n.k.), unaweza kutumia aikoni iliyounganishwa na mtindo wa kuelea kwa kuongeza `.Ico` jina la darasa kwenye chombo cha nje au sehemu yenyewe.

### Mfano wa muundo

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">hariri</div>
  </c-pophover>
</b>
```

Mpangilio msingi, saizi (18x18px), na mabadiliko ya rangi ya kielee juu yameunganishwa katika `.Ico > c-pophover > a`. Wasanidi wanahitaji tu kubainisha faili ya ikoni inayolingana kando, kama vile:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```