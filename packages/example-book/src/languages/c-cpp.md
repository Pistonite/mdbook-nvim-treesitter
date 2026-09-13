# C and C++

C++'s queries begin with `; inherits: c`, so the C patterns have to be loaded
and prepended before anything C-shaped in a C++ sample is highlighted.

## C

```c
#include <stdio.h>
#include <stdlib.h>

#define MAX_ITEMS 64
#define SQUARE(x) ((x) * (x))

typedef struct Point {
    double x, y;
} Point;

enum Status { OK = 0, FAILED = 1 };

static enum Status sum(const int *values, size_t count, long *out) {
    if (values == NULL || out == NULL) {
        return FAILED;
    }
    long total = 0L;
    for (size_t i = 0; i < count; ++i) {
        total += values[i];
    }
    *out = total;
    return OK;
}

int main(void) {
    int values[] = {1, 2, 3};
    long total = 0;
    /* A block comment, and a character literal. */
    char sep = '\t';
    if (sum(values, 3, &total) != OK) {
        return EXIT_FAILURE;
    }
    printf("total%c%ld squared%c%d\n", sep, total, sep, SQUARE(3));
    return EXIT_SUCCESS;
}
```

## C++

```cpp
#include <memory>
#include <string>
#include <vector>

namespace geometry {

template <typename T>
class Buffer {
  public:
    explicit Buffer(std::size_t capacity) : m_data(capacity) {}

    [[nodiscard]] auto size() const noexcept -> std::size_t { return m_data.size(); }

    void push(T value) { m_data.push_back(std::move(value)); }

  private:
    std::vector<T> m_data;
};

struct Point final {
    double x{0.0};
    double y{0.0};

    constexpr auto norm2() const -> double { return x * x + y * y; }
};

}  // namespace geometry

int main() {
    using namespace geometry;
    auto buffer = std::make_unique<Buffer<Point>>(4);
    buffer->push(Point{.x = 3.0, .y = 4.0});
    const auto& label = "done";
    return buffer->size() == 1 && label != nullptr ? 0 : 1;
}
```
