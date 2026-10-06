# Agent tasks from HumanEval

Read from openai/human-eval at `463c980b59e818ace59f6f9803cd92c749ceae61`, `data/HumanEval.jsonl.gz`, SHA-256 `b796127e635a67f93fb35c04f4cb03cf06f38c8072ee7cee8833d7bee06979ef`.

| problems read | kept | refused | cases recorded |
|---:|---:|---:|---:|
| 164 | 157 | 7 | 1272 |

## Refused, by reason

| reason | problems |
|---|---:|
| literal | 7 |

| problem | why |
|---|---|
| humaneval-22 | literal: int and {_: _} have no common lotml type |
| humaneval-78 | literal: str and [_] have no common lotml type |
| humaneval-95 | literal: str and int have no common lotml type |
| humaneval-103 | literal: str and int have no common lotml type |
| humaneval-125 | literal: [str] and int have no common lotml type |
| humaneval-137 | literal: int and str have no common lotml type |
| humaneval-148 | literal: (str, str) and (str,) have no common lotml type |

## Cases per kept problem

| problem | cases |
|---|---:|
| humaneval-0 | 7 |
| humaneval-1 | 4 |
| humaneval-2 | 3 |
| humaneval-3 | 6 |
| humaneval-4 | 3 |
| humaneval-5 | 3 |
| humaneval-6 | 3 |
| humaneval-7 | 4 |
| humaneval-8 | 5 |
| humaneval-9 | 4 |
| humaneval-10 | 5 |
| humaneval-11 | 3 |
| humaneval-12 | 3 |
| humaneval-13 | 4 |
| humaneval-14 | 3 |
| humaneval-15 | 3 |
| humaneval-16 | 5 |
| humaneval-17 | 5 |
| humaneval-18 | 4 |
| humaneval-19 | 5 |
| humaneval-20 | 5 |
| humaneval-21 | 5 |
| humaneval-23 | 3 |
| humaneval-24 | 5 |
| humaneval-25 | 8 |
| humaneval-26 | 3 |
| humaneval-27 | 3 |
| humaneval-28 | 3 |
| humaneval-29 | 2 |
| humaneval-30 | 4 |
| humaneval-31 | 12 |
| humaneval-32 | 50 |
| humaneval-33 | 7 |
| humaneval-34 | 1 |
| humaneval-35 | 2 |
| humaneval-36 | 8 |
| humaneval-37 | 3 |
| humaneval-38 | 50 |
| humaneval-39 | 10 |
| humaneval-40 | 9 |
| humaneval-41 | 5 |
| humaneval-42 | 3 |
| humaneval-43 | 9 |
| humaneval-44 | 12 |
| humaneval-45 | 3 |
| humaneval-46 | 4 |
| humaneval-47 | 5 |
| humaneval-48 | 7 |
| humaneval-49 | 7 |
| humaneval-50 | 50 |
| humaneval-51 | 7 |
| humaneval-52 | 6 |
| humaneval-53 | 50 |
| humaneval-54 | 7 |
| humaneval-55 | 5 |
| humaneval-56 | 12 |
| humaneval-57 | 8 |
| humaneval-58 | 4 |
| humaneval-59 | 5 |
| humaneval-60 | 5 |
| humaneval-61 | 12 |
| humaneval-62 | 5 |
| humaneval-63 | 7 |
| humaneval-64 | 7 |
| humaneval-65 | 5 |
| humaneval-66 | 8 |
| humaneval-67 | 7 |
| humaneval-68 | 8 |
| humaneval-69 | 25 |
| humaneval-70 | 9 |
| humaneval-71 | 9 |
| humaneval-72 | 6 |
| humaneval-73 | 8 |
| humaneval-74 | 9 |
| humaneval-75 | 10 |
| humaneval-76 | 10 |
| humaneval-77 | 8 |
| humaneval-79 | 4 |
| humaneval-80 | 8 |
| humaneval-81 | 6 |
| humaneval-82 | 16 |
| humaneval-83 | 5 |
| humaneval-84 | 5 |
| humaneval-85 | 4 |
| humaneval-86 | 7 |
| humaneval-87 | 6 |
| humaneval-88 | 7 |
| humaneval-89 | 8 |
| humaneval-90 | 6 |
| humaneval-91 | 6 |
| humaneval-92 | 10 |
| humaneval-93 | 5 |
| humaneval-94 | 9 |
| humaneval-96 | 10 |
| humaneval-97 | 8 |
| humaneval-98 | 7 |
| humaneval-99 | 5 |
| humaneval-100 | 5 |
| humaneval-101 | 6 |
| humaneval-102 | 8 |
| humaneval-104 | 4 |
| humaneval-105 | 5 |
| humaneval-106 | 4 |
| humaneval-107 | 8 |
| humaneval-108 | 8 |
| humaneval-109 | 5 |
| humaneval-110 | 7 |
| humaneval-111 | 7 |
| humaneval-112 | 9 |
| humaneval-113 | 3 |
| humaneval-114 | 12 |
| humaneval-115 | 5 |
| humaneval-116 | 7 |
| humaneval-117 | 7 |
| humaneval-118 | 13 |
| humaneval-119 | 12 |
| humaneval-120 | 11 |
| humaneval-121 | 7 |
| humaneval-122 | 5 |
| humaneval-123 | 4 |
| humaneval-124 | 16 |
| humaneval-126 | 13 |
| humaneval-127 | 8 |
| humaneval-128 | 8 |
| humaneval-129 | 11 |
| humaneval-130 | 10 |
| humaneval-131 | 7 |
| humaneval-132 | 14 |
| humaneval-133 | 12 |
| humaneval-134 | 10 |
| humaneval-135 | 5 |
| humaneval-136 | 11 |
| humaneval-138 | 8 |
| humaneval-139 | 4 |
| humaneval-140 | 5 |
| humaneval-141 | 26 |
| humaneval-142 | 11 |
| humaneval-143 | 7 |
| humaneval-144 | 12 |
| humaneval-145 | 6 |
| humaneval-146 | 7 |
| humaneval-147 | 4 |
| humaneval-149 | 7 |
| humaneval-150 | 10 |
| humaneval-151 | 7 |
| humaneval-152 | 4 |
| humaneval-153 | 9 |
| humaneval-154 | 6 |
| humaneval-155 | 8 |
| humaneval-156 | 14 |
| humaneval-157 | 11 |
| humaneval-158 | 10 |
| humaneval-159 | 6 |
| humaneval-160 | 3 |
| humaneval-161 | 8 |
| humaneval-162 | 4 |
| humaneval-163 | 4 |

## Notice

> The MIT License
>
> Copyright (c) OpenAI (https://openai.com)
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
> IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
> FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
> AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
> LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
> OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
> THE SOFTWARE.
