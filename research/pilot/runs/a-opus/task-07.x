# Task 7: ledger with withdraw, deposit and transfer

type LedgerErr = NoAccount(name: str) | Insufficient(name: str, balance: int, amount: int)

type Ledger(balances: {str: int})

impl Ledger:
    fn deposit(var self, name: str, amount: int) -> none ! LedgerErr:
        balance = self.balances.get(name) or fail NoAccount(name)
        self.balances[name] = balance + amount

    fn withdraw(var self, name: str, amount: int) -> none ! LedgerErr:
        balance = self.balances.get(name) or fail NoAccount(name)
        if balance < amount:
            fail Insufficient(name, balance, amount)
        self.balances[name] = balance - amount

    fn transfer(var self, src: str, dst: str, amount: int) -> none ! LedgerErr:
        # check the destination first so a failed deposit never loses money
        if not (dst in self.balances):
            fail NoAccount(dst)
        self.withdraw(src, amount)?
        self.deposit(dst, amount)?

test "deposit and withdraw":
    var ledger = Ledger({"ana": 100})
    ledger.deposit("ana", 50)?
    ledger.withdraw("ana", 30)?
    assert ledger.balances.get("ana", 0) == 120

test "withdraw fails on missing account and insufficient balance":
    var ledger = Ledger({"ana": 10})
    assert ledger.withdraw("zoe", 5) == fail NoAccount("zoe")
    assert ledger.withdraw("ana", 11) == fail Insufficient("ana", 10, 11)
    assert ledger.balances.get("ana", 0) == 10

test "successful transfer":
    var ledger = Ledger({"ana": 100, "bob": 50})
    ledger.transfer("ana", "bob", 30)?
    assert ledger.balances.get("ana", 0) == 70
    assert ledger.balances.get("bob", 0) == 80

test "failed transfer for insufficient balance leaves balances unchanged":
    var ledger = Ledger({"ana": 100, "bob": 50})
    assert ledger.transfer("ana", "bob", 150) == fail Insufficient("ana", 100, 150)
    assert ledger.balances.get("ana", 0) == 100
    assert ledger.balances.get("bob", 0) == 50

test "failed transfer to a missing account leaves balances unchanged":
    var ledger = Ledger({"ana": 100})
    assert ledger.transfer("ana", "zoe", 10) == fail NoAccount("zoe")
    assert ledger.balances.get("ana", 0) == 100

test "failed transfer from a missing account":
    var ledger = Ledger({"bob": 50})
    assert ledger.transfer("zoe", "bob", 10) == fail NoAccount("zoe")
    assert ledger.balances.get("bob", 0) == 50
