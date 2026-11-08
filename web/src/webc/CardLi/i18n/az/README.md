# Kart siyahısını asinxron şəkildə yükləyin və göstərin

- **Asinxron yükləmə**: Asinxron funksiya vasitəsilə məlumat əldə edin
- **Status Ekranı**: Üç vəziyyəti dəstəkləyir: yükləmə, boş məlumat və göstərmə kartı.
- **Flexible Layout**: Kartlar cərgələrdə düzülüb

## Demodan istifadə edin

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Başlıq" + i, "Təsvir" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## İnterfeys parametrləri

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, asinxron məlumat yükləmə funksiyası. Hər alt açarın formatı `[ad, giriş, keçid]`-dir.

## stil sinfi

- `.CardLi`: xarici element
- `.Card`: kart elementi
- `.Ing`: Animasiya elementi yüklənir