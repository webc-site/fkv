# ড্ৰপ-ডাউন নিৰ্বাচন বাকচ আৰু বুটামৰ সংমিশ্ৰণ

- নিৰ্বাচন বাকচ আৰু বুটাম অনুভূমিকভাৱে স্প্লাইচ কৰক
- নিৰ্বাচিত অৱস্থা স্বয়ংক্ৰিয়ভাৱে localStorage লে সংৰক্ষণ কৰক
- বুটাম ক্লিকসমূহ আটক কৰক আৰু স্বনিৰ্বাচিত ইভেন্টসমূহ প্ৰেৰণ কৰক

## ডেমো ব্যৱহাৰ কৰক

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">বিকল্প ১</option>
    <option value="option2">বিকল্প ২</option>
  </c-select>
  <button>দৌৰা</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## সম্পত্তি

- `key`: localStorage স্থায়িত্বৰ বাবে ব্যৱহৃত চাবিৰ নাম
- `value`: বৰ্তমানে নিৰ্বাচিত মান

## কাৰ্যক্ৰম

- `submit`: বুটাম ক্লিক কৰাৰ সময়ত প্ৰেৰণ কৰা হয়, `e.value` এ বৰ্তমানে নিৰ্বাচিত মান কঢ়িয়াই লৈ ফুৰে