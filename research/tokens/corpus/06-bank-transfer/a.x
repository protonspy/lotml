type Bank(balances: {str: int})
type BankErr = UnknownAccount(account: str) | InsufficientFunds(account: str)

impl Bank:
    fn withdraw(var self, account: str, amount: int) -> none ! BankErr:
        balance = self.balances.get(account) or fail UnknownAccount(account)
        if balance < amount:
            fail InsufficientFunds(account)
        self.balances[account] = balance - amount

    fn deposit(var self, account: str, amount: int) -> none ! BankErr:
        balance = self.balances.get(account) or fail UnknownAccount(account)
        self.balances[account] = balance + amount

    fn transfer(var self, source: str, target: str, amount: int) -> none ! BankErr:
        if target not in self.balances:
            fail UnknownAccount(target)
        self.withdraw(source, amount)?
        self.deposit(target, amount)?

test "transfer":
    var bank = Bank({"a": 100, "b": 0})
    bank.transfer("a", "b", 30)?
    assert bank.balances == {"a": 70, "b": 30}
    assert bank.transfer("b", "a", 50) == fail InsufficientFunds("b")
