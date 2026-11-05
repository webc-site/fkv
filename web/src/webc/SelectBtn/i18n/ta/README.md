# கீழ்தோன்றும் தேர்வு பெட்டி மற்றும் பொத்தானின் சேர்க்கை

- தேர்வு பெட்டி மற்றும் பொத்தானை கிடைமட்டமாக பிரிக்கவும்
- தேர்ந்தெடுக்கப்பட்ட நிலையை லோக்கல் ஸ்டோரேஜில் தானாகவே சேமிக்கவும்
- பொத்தான் கிளிக்குகளை இடைமறித்து தனிப்பயன் நிகழ்வுகளை அனுப்பவும்

## டெமோவைப் பயன்படுத்தவும்

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">விருப்பம் 1</option>
    <option value="option2">விருப்பம் 2</option>
  </c-select>
  <button>ஓடு</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## சொத்து

- `key`: உள்ளூர் சேமிப்பக நிலைத்தன்மைக்கு பயன்படுத்தப்படும் முக்கிய பெயர்
- `value`: தற்போது தேர்ந்தெடுக்கப்பட்ட மதிப்பு

## நிகழ்வு

- `submit`: பொத்தானைக் கிளிக் செய்யும் போது அனுப்பப்பட்டது, `e.value` தற்போது தேர்ந்தெடுக்கப்பட்ட மதிப்பைக் கொண்டுள்ளது