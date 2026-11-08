# 드롭다운 선택 항목이 있는 탭 페이지

- 현재 탭 페이지가 활성화된 경우에만 표시되는 적응형 너비 드롭다운 선택 상자(`<c-select>`)를 레이블에 포함할 수 있도록 지원합니다.
- 구조적 사양: 하위 요소는 `nav > a`을 라벨 탐색으로 사용하고, 직접 하위 요소인 `b`는 콘텐츠 영역으로 사용됩니다.
- `a[value]` 태그, `a`를 `c-select`(그룹의 값 속성 및 선택한 옵션 값에 따라) 및 `b[slot]` 콘텐츠 노드와 자동으로 연결합니다.
- `key` 속성을 ​​통해 현재 활성화된 항목을 `localStorage`에 자동으로 유지하는 기능을 지원합니다.
- `key + '/' + tab`을 사용하여 각 그룹의 과거 하위 선택 항목을 `localStorage`에 자동으로 저장하고 복원하는 기능을 지원합니다.
- `value` 속성을 ​​통해 활성 탭 지정을 지원합니다.

## 데모 사용

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>언어</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>체계</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">자바스크립트는 스크립팅 언어이다</b>
    <b slot="rs">Rust는 시스템 수준의 프로그래밍 언어입니다.</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS는 Unix 기반 그래픽 운영 체제입니다.</b>
    <b slot="win">Windows는 Microsoft에서 개발한 운영 체제입니다.</b>
  </b>
</c-tab-select>
```

## 재산

- `key`: localStorage 지속성에 사용되는 키 이름
- `value`: 현재 선택된 값

## 맞춤 이벤트

- `change`: 탭을 전환할 때 트리거됩니다. `e.value`는 현재 활성화된 옵션의 `value` 값입니다.