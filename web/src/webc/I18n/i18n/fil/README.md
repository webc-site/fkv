# Mga button at pop-up window para sa paglipat ng mga wika ng interface

- I-click ang pindutan upang i-pop up ang lumulutang na layer ng pagpili ng wika.
- Awtomatikong i-highlight ang kasalukuyang napiling wika.

## Gamitin ang demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Gamitin ang sumusunod na JS code upang subaybayan ang mga pagbabago sa wika
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Mga parameter ng interface

### Custom na tag na `<c-i18n>`

Mga custom na HTML tag, na ipinakilala sa JS at ginamit sa HTML.

## Pamamahala ng katayuan

Gamitin ang `src/js/i18n.js` upang pamahalaan ang katayuan ng pandaigdigang wika:

- **`LANG_LI`**: Array ng listahan ng wika, sa format na `[[name, ID], ...]`.
- **`lang()`**: Kunin ang kasalukuyang language ID.
- **`langCode()`**: Kunin ang kasalukuyang encoding ng wika.
- **`langSet(id)`**: Itakda ang language ID at abisuhan ang mga subscriber.
- **`onLang(func)`**: Mag-subscribe sa mga pagbabago sa wika. Kung naitakda na ang wika, agad na ma-trigger ang callback. Ibalik ang unsubscribe function.

## klase ng istilo

### `.BtnC.lang`

pindutan ng icon.

### `.I18n.Lg`

Ang pangunahing layout ng pop-up window ay ginagamit upang ibalot ang mga opsyon sa wika.