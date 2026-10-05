from dataclasses import dataclass

import pytest


class UnknownAccount(Exception):
    pass


class InsufficientFunds(Exception):
    pass


@dataclass
class Bank:
    balances: dict[str, int]

    def withdraw(self, account: str, amount: int) -> None:
        if account not in self.balances:
            raise UnknownAccount(account)
        if self.balances[account] < amount:
            raise InsufficientFunds(account)
        self.balances[account] -= amount

    def deposit(self, account: str, amount: int) -> None:
        if account not in self.balances:
            raise UnknownAccount(account)
        self.balances[account] += amount

    def transfer(self, source: str, target: str, amount: int) -> None:
        if target not in self.balances:
            raise UnknownAccount(target)
        self.withdraw(source, amount)
        self.deposit(target, amount)


def test_transfer() -> None:
    bank = Bank({"a": 100, "b": 0})
    bank.transfer("a", "b", 30)
    assert bank.balances == {"a": 70, "b": 30}
    with pytest.raises(InsufficientFunds):
        bank.transfer("b", "a", 50)
