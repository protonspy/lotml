# Pilot tasks

Write each solution as a complete X program. Every task asks for at least one `test` block.

1. Define a record for a 2D point with float coordinates and a function that returns the
   Euclidean distance between two points.
2. Define a type with three kinds of shapes — circle (radius), square (side) and rectangle
   (width, height) — and functions that compute the area and the perimeter of any shape.
3. Parse a positive integer from a string. It fails when the string is empty, when it is not
   a number (keep the text), and when the number is negative (keep the value). Test success
   and each kind of failure.
4. A user has a name, an age and an optional email. Write a function that returns the email
   of the first user with a given name, or nothing when there is no such user or the user
   has no email.
5. Write a generic first-in-first-out queue with enqueue, dequeue (which returns nothing when
   the queue is empty) and size.
6. Given a text, return the k most frequent lowercase words with their counts, most frequent
   first.
7. A ledger maps account names to integer balances. Write withdraw, deposit and transfer.
   Withdraw and transfer fail when an account does not exist or the balance is insufficient,
   and transfer reuses withdraw and deposit. Test a failed transfer.
8. Define an interface with a method `describe` that returns a string, implemented by a dog
   (name) and a car (model, year). Write a function that joins the descriptions of a mixed
   list with newlines.
9. An arithmetic expression is a number, the sum of two expressions, the product of two
   expressions, or the negation of an expression. Write a function that evaluates it and one
   that renders it as text with parentheses.
10. Parse text with one `name,age` pair per line into a list of records, skipping blank
    lines. Fail with the line number when a line does not have exactly two fields or when
    the age is not an integer.
