# Arayüz dillerini değiştirmek için düğmeler ve açılır pencereler

- Dil seçimi kayan katmanını açmak için düğmeye tıklayın.
- Seçili olan dili otomatik olarak vurgulayın.

## Demoyu kullanın

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Dil değişikliklerini izlemek için aşağıdaki JS kodunu kullanın
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Arayüz parametreleri

### Özel etiket `<c-i18n>`

JS'de tanıtılan ve HTML'de kullanılan özel HTML etiketleri.

## Durum yönetimi

Genel dil durumunu yönetmek için `src/js/i18n.js` kullanın:

- **`LANG_LI`**: Dil listesi dizisi, `[[name, ID], ...]` biçiminde.
- **`lang()`**: Geçerli dil kimliğini alın.
- **`langCode()`**: Geçerli dil kodlamasını alın.
- **`langSet(id)`**: Dil kimliğini ayarlayın ve aboneleri bilgilendirin.
- **`onLang(func)`**: Dil değişikliklerine abone olun. Dil ayarlanmışsa geri arama hemen tetiklenecektir. Abonelikten çıkma işlevini geri getirin.

## stil sınıfı

### `.BtnC.lang`

simge düğmesi.

### `.I18n.Lg`

Açılır pencerenin ana düzeni dil seçeneklerini sarmak için kullanılır.