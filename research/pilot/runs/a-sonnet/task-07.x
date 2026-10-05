type LedgerErr = NoAccount(name: str) | Insufficient(name: str, balance: int, needed: int)

type Ledger(balances: {str: int})

impl Ledger:
    fn withdraw(var self, name: str, amount: int) -> none ! LedgerErr:
        bal = self.balances.get(name) or fail NoAccount(name)
        if bal < amount:
            fail Insufficient(name, bal, amount)
        self.balances[name] = bal - amount
        return none

    fn deposit(var self, name: str, amount: int):
        self.balances[name] = self.balances.get(name, 0) + amount

    fn transfer(var self, src: str, dst: str, amount: int) -> none ! LedgerErr:
        if self.balances.get(dst) == none:
            fail NoAccount(dst)
        self.withdraw(src, amount)?
        self.deposit(dst, amount)
        return none

test "withdraw and deposit":
    var l = Ledger({"ana": 100})
    l.withdraw("ana", 30)?
    assert l.balances.get("ana", 0) == 70
    l.deposit("ana", 5)
    assert l.balances.get("ana", 0) == 75
    assert l.withdraw("ana", 80) == fail Insufficient("ana", 75, 80)
    assert l.withdraw("zed", 1) == fail NoAccount("zed")

test "successful transfer":
    var l = Ledger({"ana": 100, "bob": 50})
    l.transfer("ana", "bob", 40)?
    assert l.balances.get("ana", 0) == 60
    assert l.balances.get("bob", 0) == 90

test "failed transfer leaves the balances unchanged":
    var l = Ledger({"ana": 100, "bob": 50})
    assert l.transfer("ana", "bob", 500) == fail Insufficient("ana", 100, 500)
    assert l.transfer("ana", "zed", 10) == fail NoAccount("zed")
    assert l.transfer("zed", "ana", 10) == fail NoAccount("zed")
    assert l.balances.get("ana", 0) == 100
    assert l.balances.get("bob", 0) == 50
