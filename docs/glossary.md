# Glossary

One canonical term per concept, and the synonyms nobody should use for it. One entry per
line: the term in bold, the definition after an em dash, and an optional `Avoid:` list.
Every avoided synonym is reported wherever it appears as a whole word under `docs/`.

- **lotml** — a linguagem de programação orientada a LLMs que este repositório estuda; o piloto e o corpus de `research/` a chamam pelo nome provisório X, com extensão `.x`. Avoid: linguagem X
- **variante A** — a sintaxe do lotml exatamente como o estudo original a propõe: `none`, braços de `match` sem `case`, `=>`, `use`, `or` para opcionais.
- **variante B** — a variante A com as construções trocadas pelas do Python onde a semântica coincide: `None`, `case`, `lambda`, `from … import`, `??`, `is None`.
- **prior de treino** — o que um modelo já sabe de uma construção por tê-la visto no pré-treino; é o que faz uma construção idêntica à do Python sair certa sem instrução.
- **vazamento de Python** — uma construção válida em Python e inválida na variante em uso que aparece em código gerado em lotml.
- **corpus pareado** — os programas equivalentes em Python tipado, variante A e variante B, em `research/tokens/corpus/`, usados para medir tokens.
- **compilador semântico** — o compilador do lotml tratado como ferramenta que um agente chama em loop: diagnósticos estruturados, correções aplicáveis, digest e consultas.
- **digest** — a saída do compilador semântico que lista só as assinaturas públicas, tipos e documentação de um módulo, para servir de índice do projeto no contexto do modelo.
- **decodificação restrita** — geração em que o motor de inferência só aceita tokens que mantêm a saída válida segundo uma gramática ou um verificador de tipos.
- **semântica de valor** — modelo em que atribuir ou passar um valor equivale a copiá-lo; o compilador troca a cópia por move ou empréstimo quando prova que é seguro.
- **concorrência sem cor** — concorrência em que nenhuma função é marcada `async` nem exige `await`; as tarefas rodam em green threads.
- **harness de avaliação** — o conjunto de tarefas, modelos e métricas que decide as questões de sintaxe por medição; é o primeiro entregável do roadmap.
- **portão** — critério medido pelo harness de avaliação que precisa passar para a fase seguinte do roadmap começar.
