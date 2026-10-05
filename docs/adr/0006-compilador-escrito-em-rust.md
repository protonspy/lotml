---
status: proposed
---

# 0006 · Compilador escrito em Rust

## Context

O estudo original recomenda escrever o compilador em Rust, com parser descendente recursivo à
mão (mensagens de erro melhores que as de geradores de parser), arquitetura incremental por
queries com o Salsa e gramática tree-sitter separada para editores. O Salsa sustenta o
rust-analyzer com a invariante que a meta de checagem em menos de 100 ms exige: editar o corpo
de uma função não invalida dados globais. A linguagem de implementação é das decisões mais caras
de trocar: o Roc levou 487 dias para reescrever cerca de 300 mil linhas de Rust em Zig. Ver
[[estrategia-de-transpilacao]].

## Decision

Escrever o compilador em Rust, com parser escrito à mão, Salsa para a incrementalidade, Cranelift
para builds de debug e LLVM para release quando o backend nativo chegar. Rejeitados por ora: Zig
(compilação do próprio compilador mais rápida, mas ecossistema menor para LSP, Salsa e
Cranelift) e escrever o compilador na própria linguagem X (não existe ainda).

## Consequences

- O motivo do Roc para sair do Rust foi o tempo de reconstrução do próprio compilador (3,4 s
  contra cerca de 35 ms no Zig, em 450 mil linhas): esse custo recai sobre quem desenvolve o
  compilador e precisa ser vigiado desde o início, com a base dividida em crates.
- O Salsa ainda se rotula "experimental" (0.28.5); a API pode mudar.
- O ganho real do Cranelift no rustc foi modesto (cerca de 5% de um build limpo); a velocidade
  do loop do agente depende mais do frontend incremental que do backend.
