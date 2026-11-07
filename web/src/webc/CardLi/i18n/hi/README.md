# कार्ड सूची को अतुल्यकालिक रूप से लोड और प्रदर्शित करें

- **एसिंक्रोनस लोडिंग**: एसिंक्रोनस फ़ंक्शन के माध्यम से डेटा प्राप्त करें
- **स्थिति प्रदर्शन**: तीन स्थितियों का समर्थन करता है: लोडिंग, खाली डेटा और रेंडरिंग कार्ड।
- **लचीला लेआउट**: कार्ड पंक्तियों में व्यवस्थित होते हैं

## डेमो का प्रयोग करें

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["शीर्षक " + i, "विवरण " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## इंटरफ़ेस पैरामीटर

### CardLi(loader)

- **लोडर**: `() => Promise<Array<[string, string, string]>>`, एसिंक्रोनस डेटा लोडिंग फ़ंक्शन। प्रत्येक उपकुंजी का प्रारूप <b class='CODE'>[नाम, परिचय, लिंक]` है।

## शैली वर्ग

- `.CardLi`: बाहरी तत्व
- `.Card`: कार्ड तत्व
- `.Ing`: एनिमेटेड तत्व लोड हो रहा है