# लोड करा आणि कार्ड सूची असिंक्रोनसपणे प्रदर्शित करा

- **असिंक्रोनस लोडिंग**: एसिंक्रोनस फंक्शनद्वारे डेटा मिळवा
- **स्थिती डिस्प्ले**: तीन अवस्थांना सपोर्ट करते: लोडिंग, रिकामा डेटा आणि रेंडरिंग कार्ड.
- **लवचिक मांडणी**: पंक्तींमध्ये कार्डे लावली जातात

## डेमो वापरा

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["शीर्षक " + i, "वर्णन " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## इंटरफेस पॅरामीटर्स

### CardLi(loader)

- **लोडर**: `() => Promise<Array<[string, string, string]>>`, एसिंक्रोनस डेटा लोडिंग फंक्शन. प्रत्येक उपकीचे स्वरूप `[नाव, परिचय, दुवा]` आहे.

## शैली वर्ग

- `.CardLi`: बाह्य घटक
- `.Card`: कार्ड घटक
- `.Ing`: ॲनिमेटेड घटक लोड करत आहे