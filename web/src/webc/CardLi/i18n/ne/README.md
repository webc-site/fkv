# एसिंक्रोनस रूपमा कार्ड सूची लोड र प्रदर्शन गर्नुहोस्

- **एसिन्क्रोनस लोडिङ**: एसिन्क्रोनस प्रकार्य मार्फत डेटा प्राप्त गर्नुहोस्
- **स्थिति प्रदर्शन**: तीन अवस्थाहरूलाई समर्थन गर्दछ: लोडिङ, खाली डाटा, र रेन्डरिङ कार्ड।
- **लचिलो लेआउट**: कार्डहरू पङ्क्तिहरूमा व्यवस्थित छन्

## डेमो प्रयोग गर्नुहोस्

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["शीर्षक" + i, "विवरण " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## इन्टरफेस प्यारामिटरहरू

### CardLi(loader)

- **लोडर**: `() => Promise<Array<[string, string, string]>>`, एसिन्क्रोनस डाटा लोडिङ प्रकार्य। प्रत्येक उपकुञ्जीको ढाँचा `[नाम, परिचय, लिङ्क]` हो।

## शैली वर्ग

- `.CardLi`: बाहिरी तत्व
- `.Card`: कार्ड तत्व
- `.Ing`: एनिमेटेड तत्व लोड गर्दै