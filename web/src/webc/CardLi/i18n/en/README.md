# Load and display card list asynchronously

- **Asynchronous loading**: Get data through an asynchronous function
- **Status Display**: Supports three states: loading, empty data, and rendering card.
- **Flexible Layout**: Cards are arranged in rows

## Use the demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Title " + i, "Description " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Interface parameters

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, asynchronous data loading function. The format of each subkey is `[name, introduction, link]`.

## Style Class

- `.CardLi`: outer element
- `.Card`: card element
- `.Ing`: Loading animated element