type LedgerErr = NoAccount(name: str) | Insufficient(name: str, balance: int, needed: int)

type Ledger(balances: {str: int})

impl Ledger:
    fn withdraw(var self, name: str, amount: int) -> None ! LedgerErr:
        balance = self.balances.get(name) ?? fail NoAccount(name)
        if balance < amount:
            fail Insufficient(name, balance, amount)
        self.balances[name] = balance - amount
        return None

    fn deposit(var self, name: str, amount: int):
        self.balances[name] = self.balances.get(name, 0) + amount

    fn transfer(var self, src: str, dst: str, amount: int) -> None ! LedgerErr:
        if self.balances.get(dst) is None:
            fail NoAccount(dst)
        self.withdraw(src, amount)?
        self.deposit(dst, amount)
        return None

test "withdraw reduces the balance":
    var l = Ledger({"ana": 100, "bob": 20})
    l.withdraw("ana", 30)?
    assert l.balances["ana"] == 70

test "withdraw fails for a missing account or insufficient funds":
    var l = Ledger({"ana": 100})
    assert l.withdraw("zed", 1) == Err(NoAccount("zed"))
    assert l.withdraw("ana", 101) == Err(Insufficient("ana", 100, 101))
    assert l.balances["ana"] == 100

test "deposit increases the balance":
    var l = Ledger({"ana": 100})
    l.deposit("ana", 25)
    assert l.balances["ana"] == 125

test "transfer moves money between accounts":
    var l = Ledger({"ana": 100, "bob": 20})
    l.transfer("ana", "bob", 40)?
    assert l.balances["ana"] == 60
    assert l.balances["bob"] == 60

test "failed transfer leaves the ledger unchanged":
    var l = Ledger({"ana": 100, "bob": 20})
    assert l.transfer("ana", "bob", 500) == Err(Insufficient("ana", 100, 500))
    assert l.transfer("ghost", "bob", 10) == Err(NoAccount("ghost"))
    assert l.transfer("ana", "zed", 10) == Err(NoAccount("zed"))
    assert l.balances["ana"] == 100
    assert l.balances["bob"] == 20
