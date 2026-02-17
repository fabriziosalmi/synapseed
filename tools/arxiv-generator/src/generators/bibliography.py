
def generate_bibtex() -> str:
    """Generates the .bib file content."""
    return r"""
@article{touvron2023llama,
  title={Llama 2: Open foundation and fine-tuned chat models},
  author={Touvron, Hugo and Martin, Louis and Stone, Kevin and Albert, Peter and Almahairi, Amjad and Babaei, Yasmine and Bashlykov, Nikolay and Batra, Soumya and Bhargava, Prajjwal and Bhosale, Shruti and others},
  journal={arXiv preprint arXiv:2307.09288},
  year={2023}
}

@article{jiang2023mistral,
  title={Mistral 7B},
  author={Jiang, Albert Q and Sablayrolles, Alexandre and Mensch, Arthur and Bamford, Chris and Devendra, Chaplot S and Lample, Guillaume and Merritt, Patrick and Rozi{\`e}re, Baptiste and Lavril, Thibaut and Wang, Thomas and others},
  journal={arXiv preprint arXiv:2310.06825},
  year={2023}
}

@article{mcp2024,
  title={Model Context Protocol: Standardizing LLM-Context Interactions},
  author={Anthropic},
  journal={https://modelcontextprotocol.io},
  year={2024}
}

@article{lewis2020retrieval,
  title={Retrieval-augmented generation for knowledge-intensive nlp tasks},
  author={Lewis, Patrick and Perez, Ethan and Piktus, Aleksandra and Petroni, Fabio and Karpukhin, Vladimir and Goyal, Naman and K{\"u}ttler, Heinrich and Lewis, Mike and Yih, Wen-tau and Rockt{\"a}schel, Tim and others},
  journal={Advances in Neural Information Processing Systems},
  volume={33},
  pages={9459--9474},
  year={2020}
}

@article{schick2024toolformer,
  title={Toolformer: Language models can teach themselves to use tools},
  author={Schick, Timo and Dwivedi-Yu, Jane and Dess{\`\i}, Roberto and Raileanu, Roberta and Lomeli, Maria and Zettlemoyer, Luke and Cancedda, Nicola and Scialom, Thomas},
  journal={Advances in Neural Information Processing Systems},
  volume={36},
  year={2024}
}

@article{yang2024qwen,
  title={Qwen technical report},
  author={Yang, An and Yang, Baosong and Hui, Binyuan and Zheng, Bo and Yu, Bowen and Zhou, Chang and Li, Chengpeng and Li, Chengyuan and Liu, Dayiheng and Huang, Fei and others},
  journal={arXiv preprint arXiv:2309.16609},
  year={2023}
}

@article{javaheripi2023slm,
  title={Phi-2: The surprising power of small language models},
  author={Javaheripi, Mojan and Bubeck, Sebastien},
  journal={Microsoft Research Blog},
  year={2023}
}

@article{jimenez2024swebench,
  title={SWE-bench: Can Language Models Resolve Real-World GitHub Issues?},
  author={Jimenez, Carlos and Yang, John and Wettig, Alexander and Yao, Shunyu and Pei, Kexin and Press, Ofir and Narasimhan, Karthik},
  journal={arXiv preprint arXiv:2310.06770},
  year={2024}
}

@article{opendevin,
  title={OpenDevin: An Open Platform for AI Software Engineers as Generalist Agents},
  author={Xingyao Wang and Bunyamin Jeen and Yan Ma},
  journal={arXiv preprint arXiv:2404.07636},
  year={2024}
}

@article{edge2024graphrag,
  title={From Local to Global: A Graph RAG Approach to Query-Focused Summarization},
  author={Edge, Darren and Trinh, Ha and Cheng, Newman and Bradley, Joshua and Chao, Alex and Mody, Apurva and Truitt, Steven and Larson, Jonathan},
  journal={arXiv preprint arXiv:2404.16130},
  year={2024}
}
"""
