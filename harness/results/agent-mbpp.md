# Agent tasks from MBPP

Read from google-research/google-research at `f82046ba5aabbbb427dbfd38a254d26bff08b533`, `mbpp/mbpp.jsonl`, SHA-256 `ccf64ceae9c5403bf50a044cb6d505bfd2a2963ee58338ba268fd65beab92a9f`.

| problems read | kept | refused | cases recorded |
|---:|---:|---:|---:|
| 974 | 859 | 115 | 2573 |

## Refused, by reason

| reason | problems |
|---|---:|
| literal | 113 |
| record | 1 |
| unsatisfiable | 1 |

| problem | why |
|---|---|
| mbpp-26 | literal: (int, int) and (int, int, int) have no common lotml type |
| mbpp-37 | literal: int and str have no common lotml type |
| mbpp-40 | literal: an object has no lotml literal |
| mbpp-65 | literal: int and [int] have no common lotml type |
| mbpp-67 | literal: 6775685320645824322581483068371419745979053216268760300 overflows int |
| mbpp-70 | literal: (int, int, int) and (int, int, int, int) have no common lotml type |
| mbpp-88 | literal: an object has no lotml literal |
| mbpp-98 | literal: (int, int, int, int, int) and (int, int, int) have no common lotml type |
| mbpp-114 | literal: (int, int, int) and (int, int) have no common lotml type |
| mbpp-115 | literal: {int} and {_: _} have no common lotml type |
| mbpp-124 | literal: a complex has no lotml literal |
| mbpp-132 | literal: (str, str, str, str, str, str, str, str, str) and (str, str, str, str, str, str) have no common lotml type |
| mbpp-140 | literal: (int, int, int) and (int, int) have no common lotml type |
| mbpp-143 | literal: ([int], [int]) and ([int], [int], [int]) have no common lotml type |
| mbpp-145 | literal: (int, int, int, int) and (int, int, int, int, int) have no common lotml type |
| mbpp-157 | literal: [f64] and str have no common lotml type |
| mbpp-160 | literal: (str, int, str, int) and str have no common lotml type |
| mbpp-180 | record: test: AssertionError |
| mbpp-193 | literal: (int, int, int, int, int, int, int, int, int) and (int, int, int, int, int, int, int, int, int, int) have no common lotml type |
| mbpp-196 | literal: (int, int) and (int,) have no common lotml type |
| mbpp-215 | literal: [int] and int have no common lotml type |
| mbpp-216 | literal: int and [int] have no common lotml type |
| mbpp-219 | literal: (int, int, int, int, int, int) and (int, int, int, int, int, int, int) have no common lotml type |
| mbpp-222 | literal: (int, int, int, int, int, int) and (int, int, str) have no common lotml type |
| mbpp-240 | literal: int and str have no common lotml type |
| mbpp-243 | literal: int and str have no common lotml type |
| mbpp-253 | literal: int and str have no common lotml type |
| mbpp-255 | literal: (str,) and (str, str) have no common lotml type |
| mbpp-262 | literal: int and str have no common lotml type |
| mbpp-265 | literal: str and int have no common lotml type |
| mbpp-278 | literal: (int, int, int, (int, int), int) and (int, int, (int, int), int) have no common lotml type |
| mbpp-284 | literal: str and int have no common lotml type |
| mbpp-294 | literal: str and int have no common lotml type |
| mbpp-297 | literal: int and [int] have no common lotml type |
| mbpp-298 | literal: int and [int] have no common lotml type |
| mbpp-301 | literal: int and {str: {str: {_: _}}} have no common lotml type |
| mbpp-310 | literal: (str, str, str, str, str, str, str, str, str) and (str, str, str, str, str) have no common lotml type |
| mbpp-317 | literal: [int] and str have no common lotml type |
| mbpp-324 | literal: (int, int, int, int, int, int) and (int, int, int, int, int) have no common lotml type |
| mbpp-333 | literal: str and int have no common lotml type |
| mbpp-341 | literal: (int, int, int, int, int) and (int, int, int, int, int, int) have no common lotml type |
| mbpp-361 | literal: [_] and str have no common lotml type |
| mbpp-367 | literal: an argument has no literal |
| mbpp-368 | literal: ((int, int), (int, int), (int, int), (int, int)) and ((int, int), (int, int), (int, int)) have no common lotml type |
| mbpp-376 | literal: (int, int, int, int, int, int, int, int, int, int) and (int, int, int, int, int, int, int, int, int, int, int) have no common lotml type |
| mbpp-390 | literal: int and str have no common lotml type |
| mbpp-394 | literal: (int, int, int, int, int, int) and (int, int, int, int) have no common lotml type |
| mbpp-398 | literal: int and str have no common lotml type |
| mbpp-405 | literal: str and int have no common lotml type |
| mbpp-407 | literal: int and bool have no common lotml type |
| mbpp-410 | literal: str and int have no common lotml type |
| mbpp-413 | literal: str and int have no common lotml type |
| mbpp-417 | literal: (str, str, str) and (str, str) have no common lotml type |
| mbpp-418 | literal: str and int have no common lotml type |
| mbpp-425 | literal: int and str have no common lotml type |
| mbpp-431 | literal: int and str have no common lotml type |
| mbpp-444 | literal: (int, int, int, int, int) and (int, int, int, int) have no common lotml type |
| mbpp-446 | literal: (str, str, str, str, str) and (int, int, int, int, int, int, int, int, int) have no common lotml type |
| mbpp-457 | literal: int and str have no common lotml type |
| mbpp-494 | literal: (int, int, int, int, int, int, int) and (int, int, int, int, int, int, int, int) have no common lotml type |
| mbpp-513 | literal: (int, int, int, int, int) and (int, int, int, int) have no common lotml type |
| mbpp-514 | literal: (int, int, int, int, int, int) and (int, int, int, int, int) have no common lotml type |
| mbpp-533 | literal: an argument has no literal |
| mbpp-538 | literal: (str, str, str, str, str, str, str, str, str) and (str, str, str, str, str, str, str) have no common lotml type |
| mbpp-539 | literal: 100000000000000000000 overflows int |
| mbpp-544 | literal: (str, str, str) and (str, str) have no common lotml type |
| mbpp-560 | literal: (int, int, int, int, int, int) and (int, int, int, int, int, int, int) have no common lotml type |
| mbpp-580 | literal: (int, (int, (int, int)), int, int) and (int, (int, (int, int))) have no common lotml type |
| mbpp-582 | literal: {int} and {_: _} have no common lotml type |
| mbpp-585 | literal: str and f64 have no common lotml type |
| mbpp-587 | literal: (int, int, int, int, int, int) and (int, int, int, int, int, int, int, int, int) have no common lotml type |
| mbpp-590 | literal: a complex has no lotml literal |
| mbpp-595 | literal: int and str have no common lotml type |
| mbpp-596 | literal: str and int have no common lotml type |
| mbpp-601 | literal: an argument has no literal |
| mbpp-612 | literal: str and int have no common lotml type |
| mbpp-614 | literal: (int, int) and (int, int, int) have no common lotml type |
| mbpp-615 | literal: (int, int, int, int) and (int, int, int) have no common lotml type |
| mbpp-653 | literal: an object has no lotml literal |
| mbpp-686 | literal: (int, int, int, int, int, int, int, int, int) and (int, int, int, int, int, int, int, int, int, int) have no common lotml type |
| mbpp-696 | literal: int and str have no common lotml type |
| mbpp-699 | literal: int and str have no common lotml type |
| mbpp-700 | literal: int and str have no common lotml type |
| mbpp-705 | literal: int and str have no common lotml type |
| mbpp-712 | literal: [int] and str have no common lotml type |
| mbpp-715 | literal: (int, int, int, int, int) and (int, int, int, int, int, int) have no common lotml type |
| mbpp-718 | literal: str and int have no common lotml type |
| mbpp-730 | literal: int and str have no common lotml type |
| mbpp-744 | unsatisfiable: error[E0204]: expected `(int, int, int, int, int?)`, found `(int, int, int, int, int)` |
| mbpp-758 | literal: int and str have no common lotml type |
| mbpp-763 | literal: (int, int, int, int, int, int) and (int, int, int, int) have no common lotml type |
| mbpp-778 | literal: int and str have no common lotml type |
| mbpp-779 | literal: int and str have no common lotml type |
| mbpp-795 | literal: str and f64 have no common lotml type |
| mbpp-808 | literal: (int, int, int, int, int) and (int, int, int, int, int, int) have no common lotml type |
| mbpp-816 | literal: the empty tuple has no lotml literal |
| mbpp-839 | literal: int and str have no common lotml type |
| mbpp-858 | literal: int and [int] have no common lotml type |
| mbpp-859 | literal: int and str have no common lotml type |
| mbpp-872 | literal: int and [int] have no common lotml type |
| mbpp-886 | literal: (int, int, int, int, int) and (int, int, int) have no common lotml type |
| mbpp-893 | literal: int and str have no common lotml type |
| mbpp-894 | literal: (f64, f64, f64, f64, f64) and (f64, f64, f64, f64) have no common lotml type |
| mbpp-902 | literal: an object has no lotml literal |
| mbpp-910 | literal: int and str have no common lotml type |
| mbpp-920 | literal: (int?, int?) and (None,) have no common lotml type |
| mbpp-921 | literal: (int, int, int, int, int, int, int, int, int) and (int, int, int, int, int, int, int, int) have no common lotml type |
| mbpp-925 | literal: (int, int, int, int, int, int) and (int, int, int) have no common lotml type |
| mbpp-927 | literal: an argument has no literal |
| mbpp-939 | literal: str and int have no common lotml type |
| mbpp-941 | literal: int and (int, int) have no common lotml type |
| mbpp-942 | literal: (int, int, int, int, int) and (int, int, int, int) have no common lotml type |
| mbpp-949 | literal: (int, int, int, int) and (int, int) have no common lotml type |
| mbpp-966 | literal: the empty tuple has no lotml literal |
| mbpp-969 | literal: (int, int, int) and (int, int) have no common lotml type |

## Cases per kept problem

| problem | cases |
|---|---:|
| mbpp-1 | 3 |
| mbpp-2 | 3 |
| mbpp-3 | 3 |
| mbpp-4 | 3 |
| mbpp-5 | 3 |
| mbpp-6 | 3 |
| mbpp-7 | 3 |
| mbpp-8 | 3 |
| mbpp-9 | 3 |
| mbpp-10 | 3 |
| mbpp-11 | 3 |
| mbpp-12 | 3 |
| mbpp-13 | 3 |
| mbpp-14 | 3 |
| mbpp-15 | 3 |
| mbpp-16 | 3 |
| mbpp-17 | 3 |
| mbpp-18 | 3 |
| mbpp-19 | 3 |
| mbpp-20 | 3 |
| mbpp-21 | 3 |
| mbpp-22 | 3 |
| mbpp-23 | 3 |
| mbpp-24 | 3 |
| mbpp-25 | 3 |
| mbpp-27 | 3 |
| mbpp-28 | 3 |
| mbpp-29 | 3 |
| mbpp-30 | 3 |
| mbpp-31 | 3 |
| mbpp-32 | 3 |
| mbpp-33 | 3 |
| mbpp-34 | 3 |
| mbpp-35 | 3 |
| mbpp-36 | 3 |
| mbpp-38 | 3 |
| mbpp-39 | 3 |
| mbpp-41 | 3 |
| mbpp-42 | 3 |
| mbpp-43 | 3 |
| mbpp-44 | 3 |
| mbpp-45 | 3 |
| mbpp-46 | 3 |
| mbpp-47 | 3 |
| mbpp-48 | 3 |
| mbpp-49 | 3 |
| mbpp-50 | 3 |
| mbpp-51 | 3 |
| mbpp-52 | 3 |
| mbpp-53 | 3 |
| mbpp-54 | 3 |
| mbpp-55 | 3 |
| mbpp-56 | 3 |
| mbpp-57 | 3 |
| mbpp-58 | 3 |
| mbpp-59 | 3 |
| mbpp-60 | 3 |
| mbpp-61 | 3 |
| mbpp-62 | 3 |
| mbpp-63 | 3 |
| mbpp-64 | 3 |
| mbpp-66 | 3 |
| mbpp-68 | 3 |
| mbpp-69 | 3 |
| mbpp-71 | 3 |
| mbpp-72 | 3 |
| mbpp-73 | 3 |
| mbpp-74 | 3 |
| mbpp-75 | 3 |
| mbpp-76 | 3 |
| mbpp-77 | 3 |
| mbpp-78 | 3 |
| mbpp-79 | 3 |
| mbpp-80 | 3 |
| mbpp-81 | 3 |
| mbpp-82 | 3 |
| mbpp-83 | 3 |
| mbpp-84 | 3 |
| mbpp-85 | 3 |
| mbpp-86 | 3 |
| mbpp-87 | 3 |
| mbpp-89 | 3 |
| mbpp-90 | 3 |
| mbpp-91 | 3 |
| mbpp-92 | 3 |
| mbpp-93 | 3 |
| mbpp-94 | 3 |
| mbpp-95 | 3 |
| mbpp-96 | 3 |
| mbpp-97 | 3 |
| mbpp-99 | 3 |
| mbpp-100 | 3 |
| mbpp-101 | 3 |
| mbpp-102 | 3 |
| mbpp-103 | 3 |
| mbpp-104 | 3 |
| mbpp-105 | 3 |
| mbpp-106 | 3 |
| mbpp-107 | 3 |
| mbpp-108 | 3 |
| mbpp-109 | 3 |
| mbpp-110 | 3 |
| mbpp-111 | 3 |
| mbpp-112 | 3 |
| mbpp-113 | 3 |
| mbpp-116 | 3 |
| mbpp-117 | 3 |
| mbpp-118 | 3 |
| mbpp-119 | 3 |
| mbpp-120 | 3 |
| mbpp-121 | 3 |
| mbpp-122 | 3 |
| mbpp-123 | 3 |
| mbpp-125 | 3 |
| mbpp-126 | 3 |
| mbpp-127 | 3 |
| mbpp-128 | 3 |
| mbpp-129 | 3 |
| mbpp-130 | 3 |
| mbpp-131 | 3 |
| mbpp-133 | 3 |
| mbpp-134 | 3 |
| mbpp-135 | 3 |
| mbpp-136 | 3 |
| mbpp-137 | 3 |
| mbpp-138 | 3 |
| mbpp-139 | 3 |
| mbpp-141 | 3 |
| mbpp-142 | 3 |
| mbpp-144 | 3 |
| mbpp-146 | 3 |
| mbpp-147 | 3 |
| mbpp-148 | 3 |
| mbpp-149 | 3 |
| mbpp-150 | 3 |
| mbpp-151 | 3 |
| mbpp-152 | 3 |
| mbpp-153 | 3 |
| mbpp-154 | 3 |
| mbpp-155 | 3 |
| mbpp-156 | 3 |
| mbpp-158 | 3 |
| mbpp-159 | 3 |
| mbpp-161 | 3 |
| mbpp-162 | 3 |
| mbpp-163 | 3 |
| mbpp-164 | 3 |
| mbpp-165 | 3 |
| mbpp-166 | 3 |
| mbpp-167 | 3 |
| mbpp-168 | 3 |
| mbpp-169 | 3 |
| mbpp-170 | 3 |
| mbpp-171 | 3 |
| mbpp-172 | 3 |
| mbpp-173 | 3 |
| mbpp-174 | 3 |
| mbpp-175 | 3 |
| mbpp-176 | 3 |
| mbpp-177 | 3 |
| mbpp-178 | 3 |
| mbpp-179 | 3 |
| mbpp-181 | 3 |
| mbpp-182 | 3 |
| mbpp-183 | 3 |
| mbpp-184 | 3 |
| mbpp-185 | 3 |
| mbpp-186 | 3 |
| mbpp-187 | 3 |
| mbpp-188 | 3 |
| mbpp-189 | 3 |
| mbpp-190 | 3 |
| mbpp-191 | 3 |
| mbpp-192 | 3 |
| mbpp-194 | 3 |
| mbpp-195 | 3 |
| mbpp-197 | 3 |
| mbpp-198 | 3 |
| mbpp-199 | 3 |
| mbpp-200 | 3 |
| mbpp-201 | 3 |
| mbpp-202 | 3 |
| mbpp-203 | 3 |
| mbpp-204 | 3 |
| mbpp-205 | 3 |
| mbpp-206 | 3 |
| mbpp-207 | 3 |
| mbpp-208 | 3 |
| mbpp-209 | 3 |
| mbpp-210 | 3 |
| mbpp-211 | 3 |
| mbpp-212 | 3 |
| mbpp-213 | 3 |
| mbpp-214 | 3 |
| mbpp-217 | 3 |
| mbpp-218 | 3 |
| mbpp-220 | 3 |
| mbpp-221 | 3 |
| mbpp-223 | 3 |
| mbpp-224 | 3 |
| mbpp-225 | 3 |
| mbpp-226 | 3 |
| mbpp-227 | 3 |
| mbpp-228 | 3 |
| mbpp-229 | 3 |
| mbpp-230 | 3 |
| mbpp-231 | 3 |
| mbpp-232 | 3 |
| mbpp-233 | 3 |
| mbpp-234 | 3 |
| mbpp-235 | 3 |
| mbpp-236 | 3 |
| mbpp-237 | 3 |
| mbpp-238 | 3 |
| mbpp-239 | 3 |
| mbpp-241 | 3 |
| mbpp-242 | 3 |
| mbpp-244 | 3 |
| mbpp-245 | 3 |
| mbpp-246 | 3 |
| mbpp-247 | 3 |
| mbpp-248 | 3 |
| mbpp-249 | 3 |
| mbpp-250 | 3 |
| mbpp-251 | 3 |
| mbpp-252 | 3 |
| mbpp-254 | 3 |
| mbpp-256 | 3 |
| mbpp-257 | 3 |
| mbpp-258 | 3 |
| mbpp-259 | 3 |
| mbpp-260 | 3 |
| mbpp-261 | 3 |
| mbpp-263 | 3 |
| mbpp-264 | 3 |
| mbpp-266 | 3 |
| mbpp-267 | 3 |
| mbpp-268 | 3 |
| mbpp-269 | 3 |
| mbpp-270 | 3 |
| mbpp-271 | 3 |
| mbpp-272 | 3 |
| mbpp-273 | 3 |
| mbpp-274 | 3 |
| mbpp-275 | 3 |
| mbpp-276 | 3 |
| mbpp-277 | 3 |
| mbpp-279 | 3 |
| mbpp-280 | 3 |
| mbpp-281 | 3 |
| mbpp-282 | 3 |
| mbpp-283 | 3 |
| mbpp-285 | 3 |
| mbpp-286 | 3 |
| mbpp-287 | 3 |
| mbpp-288 | 3 |
| mbpp-289 | 3 |
| mbpp-290 | 3 |
| mbpp-291 | 3 |
| mbpp-292 | 3 |
| mbpp-293 | 3 |
| mbpp-295 | 3 |
| mbpp-296 | 3 |
| mbpp-299 | 3 |
| mbpp-300 | 3 |
| mbpp-302 | 3 |
| mbpp-303 | 3 |
| mbpp-304 | 3 |
| mbpp-305 | 3 |
| mbpp-306 | 3 |
| mbpp-307 | 3 |
| mbpp-308 | 3 |
| mbpp-309 | 3 |
| mbpp-311 | 3 |
| mbpp-312 | 3 |
| mbpp-313 | 3 |
| mbpp-314 | 3 |
| mbpp-315 | 3 |
| mbpp-316 | 3 |
| mbpp-318 | 3 |
| mbpp-319 | 3 |
| mbpp-320 | 3 |
| mbpp-321 | 3 |
| mbpp-322 | 3 |
| mbpp-323 | 3 |
| mbpp-325 | 3 |
| mbpp-326 | 3 |
| mbpp-327 | 3 |
| mbpp-328 | 3 |
| mbpp-329 | 3 |
| mbpp-330 | 3 |
| mbpp-331 | 3 |
| mbpp-332 | 3 |
| mbpp-334 | 3 |
| mbpp-335 | 3 |
| mbpp-336 | 3 |
| mbpp-337 | 2 |
| mbpp-338 | 3 |
| mbpp-339 | 3 |
| mbpp-340 | 3 |
| mbpp-342 | 3 |
| mbpp-343 | 3 |
| mbpp-344 | 3 |
| mbpp-345 | 3 |
| mbpp-346 | 3 |
| mbpp-347 | 3 |
| mbpp-348 | 3 |
| mbpp-349 | 3 |
| mbpp-350 | 3 |
| mbpp-351 | 3 |
| mbpp-352 | 3 |
| mbpp-353 | 3 |
| mbpp-354 | 3 |
| mbpp-355 | 3 |
| mbpp-356 | 3 |
| mbpp-357 | 3 |
| mbpp-358 | 3 |
| mbpp-359 | 3 |
| mbpp-360 | 3 |
| mbpp-362 | 3 |
| mbpp-363 | 3 |
| mbpp-364 | 3 |
| mbpp-365 | 3 |
| mbpp-366 | 3 |
| mbpp-369 | 3 |
| mbpp-370 | 3 |
| mbpp-371 | 3 |
| mbpp-372 | 3 |
| mbpp-373 | 3 |
| mbpp-374 | 3 |
| mbpp-375 | 3 |
| mbpp-377 | 3 |
| mbpp-378 | 3 |
| mbpp-379 | 3 |
| mbpp-380 | 3 |
| mbpp-381 | 3 |
| mbpp-382 | 3 |
| mbpp-383 | 3 |
| mbpp-384 | 3 |
| mbpp-385 | 3 |
| mbpp-386 | 3 |
| mbpp-387 | 3 |
| mbpp-388 | 3 |
| mbpp-389 | 3 |
| mbpp-391 | 3 |
| mbpp-392 | 3 |
| mbpp-393 | 3 |
| mbpp-395 | 3 |
| mbpp-396 | 3 |
| mbpp-397 | 3 |
| mbpp-399 | 3 |
| mbpp-400 | 3 |
| mbpp-401 | 3 |
| mbpp-402 | 3 |
| mbpp-403 | 3 |
| mbpp-404 | 3 |
| mbpp-406 | 3 |
| mbpp-408 | 3 |
| mbpp-409 | 3 |
| mbpp-411 | 3 |
| mbpp-412 | 3 |
| mbpp-414 | 3 |
| mbpp-415 | 3 |
| mbpp-416 | 3 |
| mbpp-419 | 3 |
| mbpp-420 | 3 |
| mbpp-421 | 3 |
| mbpp-422 | 3 |
| mbpp-423 | 3 |
| mbpp-424 | 3 |
| mbpp-426 | 3 |
| mbpp-427 | 3 |
| mbpp-428 | 3 |
| mbpp-429 | 3 |
| mbpp-430 | 3 |
| mbpp-432 | 3 |
| mbpp-433 | 3 |
| mbpp-434 | 3 |
| mbpp-435 | 3 |
| mbpp-436 | 3 |
| mbpp-437 | 3 |
| mbpp-438 | 3 |
| mbpp-439 | 3 |
| mbpp-440 | 3 |
| mbpp-441 | 3 |
| mbpp-442 | 3 |
| mbpp-443 | 3 |
| mbpp-445 | 3 |
| mbpp-447 | 3 |
| mbpp-448 | 3 |
| mbpp-449 | 3 |
| mbpp-450 | 3 |
| mbpp-451 | 3 |
| mbpp-452 | 3 |
| mbpp-453 | 3 |
| mbpp-454 | 3 |
| mbpp-455 | 3 |
| mbpp-456 | 3 |
| mbpp-458 | 3 |
| mbpp-459 | 3 |
| mbpp-460 | 3 |
| mbpp-461 | 3 |
| mbpp-462 | 3 |
| mbpp-463 | 3 |
| mbpp-464 | 3 |
| mbpp-465 | 3 |
| mbpp-466 | 3 |
| mbpp-467 | 3 |
| mbpp-468 | 3 |
| mbpp-469 | 3 |
| mbpp-470 | 3 |
| mbpp-471 | 3 |
| mbpp-472 | 3 |
| mbpp-473 | 3 |
| mbpp-474 | 3 |
| mbpp-475 | 3 |
| mbpp-476 | 3 |
| mbpp-477 | 3 |
| mbpp-478 | 3 |
| mbpp-479 | 3 |
| mbpp-480 | 3 |
| mbpp-481 | 3 |
| mbpp-482 | 3 |
| mbpp-483 | 3 |
| mbpp-484 | 3 |
| mbpp-485 | 3 |
| mbpp-486 | 3 |
| mbpp-487 | 3 |
| mbpp-488 | 3 |
| mbpp-489 | 3 |
| mbpp-490 | 3 |
| mbpp-491 | 3 |
| mbpp-492 | 3 |
| mbpp-493 | 3 |
| mbpp-495 | 3 |
| mbpp-496 | 3 |
| mbpp-497 | 3 |
| mbpp-498 | 3 |
| mbpp-499 | 3 |
| mbpp-500 | 3 |
| mbpp-501 | 3 |
| mbpp-502 | 3 |
| mbpp-503 | 3 |
| mbpp-504 | 3 |
| mbpp-505 | 3 |
| mbpp-506 | 3 |
| mbpp-507 | 3 |
| mbpp-508 | 2 |
| mbpp-509 | 3 |
| mbpp-510 | 3 |
| mbpp-511 | 3 |
| mbpp-512 | 3 |
| mbpp-515 | 3 |
| mbpp-516 | 3 |
| mbpp-517 | 3 |
| mbpp-518 | 3 |
| mbpp-519 | 3 |
| mbpp-520 | 3 |
| mbpp-521 | 3 |
| mbpp-522 | 3 |
| mbpp-523 | 3 |
| mbpp-524 | 3 |
| mbpp-525 | 3 |
| mbpp-526 | 3 |
| mbpp-527 | 3 |
| mbpp-528 | 3 |
| mbpp-529 | 3 |
| mbpp-530 | 3 |
| mbpp-531 | 3 |
| mbpp-532 | 3 |
| mbpp-534 | 3 |
| mbpp-535 | 3 |
| mbpp-536 | 3 |
| mbpp-537 | 3 |
| mbpp-540 | 3 |
| mbpp-541 | 3 |
| mbpp-542 | 3 |
| mbpp-543 | 3 |
| mbpp-545 | 3 |
| mbpp-546 | 3 |
| mbpp-547 | 3 |
| mbpp-548 | 3 |
| mbpp-549 | 3 |
| mbpp-550 | 3 |
| mbpp-551 | 3 |
| mbpp-552 | 3 |
| mbpp-553 | 3 |
| mbpp-554 | 3 |
| mbpp-555 | 3 |
| mbpp-556 | 3 |
| mbpp-557 | 3 |
| mbpp-558 | 3 |
| mbpp-559 | 3 |
| mbpp-561 | 3 |
| mbpp-562 | 3 |
| mbpp-563 | 3 |
| mbpp-564 | 3 |
| mbpp-565 | 3 |
| mbpp-566 | 3 |
| mbpp-567 | 3 |
| mbpp-568 | 3 |
| mbpp-569 | 3 |
| mbpp-570 | 3 |
| mbpp-571 | 3 |
| mbpp-572 | 3 |
| mbpp-573 | 3 |
| mbpp-574 | 3 |
| mbpp-575 | 3 |
| mbpp-576 | 3 |
| mbpp-577 | 3 |
| mbpp-578 | 3 |
| mbpp-579 | 3 |
| mbpp-581 | 3 |
| mbpp-583 | 3 |
| mbpp-584 | 3 |
| mbpp-586 | 3 |
| mbpp-588 | 3 |
| mbpp-589 | 3 |
| mbpp-591 | 3 |
| mbpp-592 | 3 |
| mbpp-593 | 3 |
| mbpp-594 | 3 |
| mbpp-597 | 3 |
| mbpp-598 | 3 |
| mbpp-599 | 3 |
| mbpp-600 | 3 |
| mbpp-602 | 3 |
| mbpp-603 | 3 |
| mbpp-604 | 3 |
| mbpp-605 | 3 |
| mbpp-606 | 3 |
| mbpp-607 | 3 |
| mbpp-608 | 3 |
| mbpp-609 | 3 |
| mbpp-610 | 3 |
| mbpp-611 | 3 |
| mbpp-613 | 3 |
| mbpp-616 | 3 |
| mbpp-617 | 3 |
| mbpp-618 | 3 |
| mbpp-619 | 3 |
| mbpp-620 | 3 |
| mbpp-621 | 3 |
| mbpp-622 | 3 |
| mbpp-623 | 3 |
| mbpp-624 | 3 |
| mbpp-625 | 3 |
| mbpp-626 | 3 |
| mbpp-627 | 3 |
| mbpp-628 | 3 |
| mbpp-629 | 3 |
| mbpp-630 | 3 |
| mbpp-631 | 3 |
| mbpp-632 | 3 |
| mbpp-633 | 3 |
| mbpp-634 | 3 |
| mbpp-635 | 3 |
| mbpp-636 | 3 |
| mbpp-637 | 3 |
| mbpp-638 | 3 |
| mbpp-639 | 3 |
| mbpp-640 | 3 |
| mbpp-641 | 3 |
| mbpp-642 | 3 |
| mbpp-643 | 3 |
| mbpp-644 | 3 |
| mbpp-645 | 3 |
| mbpp-646 | 3 |
| mbpp-647 | 3 |
| mbpp-648 | 3 |
| mbpp-649 | 3 |
| mbpp-650 | 3 |
| mbpp-651 | 3 |
| mbpp-652 | 3 |
| mbpp-654 | 3 |
| mbpp-655 | 3 |
| mbpp-656 | 3 |
| mbpp-657 | 3 |
| mbpp-658 | 3 |
| mbpp-659 | 3 |
| mbpp-660 | 3 |
| mbpp-661 | 3 |
| mbpp-662 | 3 |
| mbpp-663 | 3 |
| mbpp-664 | 3 |
| mbpp-665 | 3 |
| mbpp-666 | 3 |
| mbpp-667 | 3 |
| mbpp-668 | 3 |
| mbpp-669 | 3 |
| mbpp-670 | 3 |
| mbpp-671 | 3 |
| mbpp-672 | 3 |
| mbpp-673 | 3 |
| mbpp-674 | 3 |
| mbpp-675 | 3 |
| mbpp-676 | 3 |
| mbpp-677 | 3 |
| mbpp-678 | 3 |
| mbpp-679 | 3 |
| mbpp-680 | 3 |
| mbpp-681 | 3 |
| mbpp-682 | 3 |
| mbpp-683 | 3 |
| mbpp-684 | 3 |
| mbpp-685 | 3 |
| mbpp-687 | 3 |
| mbpp-688 | 3 |
| mbpp-689 | 3 |
| mbpp-690 | 3 |
| mbpp-691 | 3 |
| mbpp-692 | 3 |
| mbpp-693 | 3 |
| mbpp-694 | 3 |
| mbpp-695 | 3 |
| mbpp-697 | 3 |
| mbpp-698 | 3 |
| mbpp-701 | 3 |
| mbpp-702 | 3 |
| mbpp-703 | 3 |
| mbpp-704 | 3 |
| mbpp-706 | 3 |
| mbpp-707 | 3 |
| mbpp-708 | 3 |
| mbpp-709 | 3 |
| mbpp-710 | 3 |
| mbpp-711 | 3 |
| mbpp-713 | 2 |
| mbpp-714 | 3 |
| mbpp-716 | 3 |
| mbpp-717 | 3 |
| mbpp-719 | 3 |
| mbpp-720 | 3 |
| mbpp-721 | 3 |
| mbpp-722 | 3 |
| mbpp-723 | 3 |
| mbpp-724 | 3 |
| mbpp-725 | 3 |
| mbpp-726 | 3 |
| mbpp-727 | 3 |
| mbpp-728 | 3 |
| mbpp-729 | 3 |
| mbpp-731 | 3 |
| mbpp-732 | 3 |
| mbpp-733 | 3 |
| mbpp-734 | 3 |
| mbpp-735 | 3 |
| mbpp-736 | 3 |
| mbpp-737 | 3 |
| mbpp-738 | 3 |
| mbpp-739 | 3 |
| mbpp-740 | 3 |
| mbpp-741 | 3 |
| mbpp-742 | 3 |
| mbpp-743 | 3 |
| mbpp-745 | 3 |
| mbpp-746 | 3 |
| mbpp-747 | 3 |
| mbpp-748 | 3 |
| mbpp-749 | 3 |
| mbpp-750 | 3 |
| mbpp-751 | 3 |
| mbpp-752 | 3 |
| mbpp-753 | 3 |
| mbpp-754 | 3 |
| mbpp-755 | 3 |
| mbpp-756 | 3 |
| mbpp-757 | 3 |
| mbpp-759 | 3 |
| mbpp-760 | 3 |
| mbpp-761 | 3 |
| mbpp-762 | 3 |
| mbpp-764 | 3 |
| mbpp-765 | 3 |
| mbpp-766 | 3 |
| mbpp-767 | 3 |
| mbpp-768 | 3 |
| mbpp-769 | 3 |
| mbpp-770 | 3 |
| mbpp-771 | 3 |
| mbpp-772 | 3 |
| mbpp-773 | 3 |
| mbpp-774 | 3 |
| mbpp-775 | 3 |
| mbpp-776 | 3 |
| mbpp-777 | 3 |
| mbpp-780 | 3 |
| mbpp-781 | 3 |
| mbpp-782 | 3 |
| mbpp-783 | 3 |
| mbpp-784 | 3 |
| mbpp-785 | 3 |
| mbpp-786 | 3 |
| mbpp-787 | 3 |
| mbpp-788 | 3 |
| mbpp-789 | 3 |
| mbpp-790 | 3 |
| mbpp-791 | 3 |
| mbpp-792 | 3 |
| mbpp-793 | 3 |
| mbpp-794 | 3 |
| mbpp-796 | 3 |
| mbpp-797 | 3 |
| mbpp-798 | 3 |
| mbpp-799 | 3 |
| mbpp-800 | 3 |
| mbpp-801 | 3 |
| mbpp-802 | 3 |
| mbpp-803 | 3 |
| mbpp-804 | 3 |
| mbpp-805 | 3 |
| mbpp-806 | 3 |
| mbpp-807 | 3 |
| mbpp-809 | 3 |
| mbpp-810 | 3 |
| mbpp-811 | 3 |
| mbpp-812 | 3 |
| mbpp-813 | 3 |
| mbpp-814 | 3 |
| mbpp-815 | 3 |
| mbpp-817 | 3 |
| mbpp-818 | 3 |
| mbpp-819 | 3 |
| mbpp-820 | 3 |
| mbpp-821 | 3 |
| mbpp-822 | 3 |
| mbpp-823 | 3 |
| mbpp-824 | 3 |
| mbpp-825 | 3 |
| mbpp-826 | 3 |
| mbpp-827 | 3 |
| mbpp-828 | 3 |
| mbpp-829 | 3 |
| mbpp-830 | 3 |
| mbpp-831 | 3 |
| mbpp-832 | 3 |
| mbpp-833 | 3 |
| mbpp-834 | 3 |
| mbpp-835 | 3 |
| mbpp-836 | 3 |
| mbpp-837 | 3 |
| mbpp-838 | 3 |
| mbpp-840 | 3 |
| mbpp-841 | 3 |
| mbpp-842 | 3 |
| mbpp-843 | 3 |
| mbpp-844 | 3 |
| mbpp-845 | 3 |
| mbpp-846 | 3 |
| mbpp-847 | 3 |
| mbpp-848 | 3 |
| mbpp-849 | 3 |
| mbpp-850 | 3 |
| mbpp-851 | 3 |
| mbpp-852 | 3 |
| mbpp-853 | 3 |
| mbpp-854 | 3 |
| mbpp-855 | 3 |
| mbpp-856 | 3 |
| mbpp-857 | 3 |
| mbpp-860 | 3 |
| mbpp-861 | 3 |
| mbpp-862 | 2 |
| mbpp-863 | 3 |
| mbpp-864 | 3 |
| mbpp-865 | 3 |
| mbpp-866 | 3 |
| mbpp-867 | 3 |
| mbpp-868 | 3 |
| mbpp-869 | 3 |
| mbpp-870 | 3 |
| mbpp-871 | 3 |
| mbpp-873 | 3 |
| mbpp-874 | 3 |
| mbpp-875 | 3 |
| mbpp-876 | 3 |
| mbpp-877 | 3 |
| mbpp-878 | 3 |
| mbpp-879 | 3 |
| mbpp-880 | 3 |
| mbpp-881 | 3 |
| mbpp-882 | 3 |
| mbpp-883 | 3 |
| mbpp-884 | 3 |
| mbpp-885 | 3 |
| mbpp-887 | 3 |
| mbpp-888 | 3 |
| mbpp-889 | 3 |
| mbpp-890 | 3 |
| mbpp-891 | 3 |
| mbpp-892 | 3 |
| mbpp-895 | 3 |
| mbpp-896 | 3 |
| mbpp-897 | 3 |
| mbpp-898 | 3 |
| mbpp-899 | 3 |
| mbpp-900 | 3 |
| mbpp-901 | 3 |
| mbpp-903 | 3 |
| mbpp-904 | 3 |
| mbpp-905 | 3 |
| mbpp-906 | 3 |
| mbpp-907 | 3 |
| mbpp-908 | 3 |
| mbpp-909 | 3 |
| mbpp-911 | 3 |
| mbpp-912 | 3 |
| mbpp-913 | 3 |
| mbpp-914 | 3 |
| mbpp-915 | 3 |
| mbpp-916 | 3 |
| mbpp-917 | 3 |
| mbpp-918 | 3 |
| mbpp-919 | 3 |
| mbpp-922 | 3 |
| mbpp-923 | 3 |
| mbpp-924 | 3 |
| mbpp-926 | 3 |
| mbpp-928 | 3 |
| mbpp-929 | 3 |
| mbpp-930 | 3 |
| mbpp-931 | 3 |
| mbpp-932 | 3 |
| mbpp-933 | 3 |
| mbpp-934 | 3 |
| mbpp-935 | 3 |
| mbpp-936 | 3 |
| mbpp-937 | 3 |
| mbpp-938 | 3 |
| mbpp-940 | 3 |
| mbpp-943 | 3 |
| mbpp-944 | 3 |
| mbpp-945 | 3 |
| mbpp-946 | 3 |
| mbpp-947 | 3 |
| mbpp-948 | 3 |
| mbpp-950 | 3 |
| mbpp-951 | 3 |
| mbpp-952 | 3 |
| mbpp-953 | 3 |
| mbpp-954 | 3 |
| mbpp-955 | 3 |
| mbpp-956 | 3 |
| mbpp-957 | 3 |
| mbpp-958 | 3 |
| mbpp-959 | 3 |
| mbpp-960 | 3 |
| mbpp-961 | 3 |
| mbpp-962 | 3 |
| mbpp-963 | 3 |
| mbpp-964 | 3 |
| mbpp-965 | 3 |
| mbpp-967 | 3 |
| mbpp-968 | 3 |
| mbpp-970 | 3 |
| mbpp-971 | 3 |
| mbpp-972 | 3 |
| mbpp-973 | 3 |
| mbpp-974 | 3 |

## Notice

> MBPP (Mostly Basic Python Problems), from "Program Synthesis with Large Language Models",
> Austin et al., 2021, released by Google Research under the Creative Commons Attribution 4.0
> International licence (CC BY 4.0, https://creativecommons.org/licenses/by/4.0/). The problems are
> posed here as lotml agent tasks: their text as a docstring, their tests' recorded values as
> hidden test blocks.
