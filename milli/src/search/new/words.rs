use super::logger::SearchLogger;
use super::resolve_query_graph::resolve_query_graph;
use super::{QueryGraph, QueryNode, RankingRule, RankingRuleOutput, SearchContext};
use crate::{Result, TermsMatchingStrategy};
use roaring::RoaringBitmap;
use std::collections::BTreeSet;

pub struct Words {
    exhausted: bool,
    query_graph: Option<QueryGraph>,
    iterating: bool,
    positions_to_remove: Vec<i8>,
    terms_matching_strategy: TermsMatchingStrategy,
}
impl Words {
    pub fn new(terms_matching_strategy: TermsMatchingStrategy) -> Self {
        Self {
            exhausted: true,
            query_graph: None,
            iterating: false,
            positions_to_remove: vec![],
            terms_matching_strategy,
        }
    }
}

impl<'search> RankingRule<'search, QueryGraph> for Words {
    fn id(&self) -> String {
        "words".to_owned()
    }
    fn start_iteration(
        &mut self,
        _ctx: &mut SearchContext<'search>,
        _logger: &mut dyn SearchLogger<QueryGraph>,
        _parent_candidates: &RoaringBitmap,
        parent_query_graph: &QueryGraph,
    ) -> Result<()> {
        self.exhausted = false;
        self.query_graph = Some(parent_query_graph.clone());

        let positions_to_remove = match self.terms_matching_strategy {
            TermsMatchingStrategy::Last => {
                let mut all_positions = BTreeSet::new();
                for n in parent_query_graph.nodes.iter() {
                    match n {
                        QueryNode::Term(term) => {
                            all_positions.extend(term.positions.clone().into_iter());
                        }
                        QueryNode::Deleted | QueryNode::Start | QueryNode::End => {}
                    }
                }
                let mut r: Vec<i8> = all_positions.into_iter().collect();
                // don't remove the first term
                r.remove(0);
                r
            }
            TermsMatchingStrategy::All => vec![],
        };
        self.positions_to_remove = positions_to_remove;
        self.iterating = true;
        Ok(())
    }

    fn next_bucket(
        &mut self,
        ctx: &mut SearchContext<'search>,
        logger: &mut dyn SearchLogger<QueryGraph>,
        universe: &RoaringBitmap,
    ) -> Result<Option<RankingRuleOutput<QueryGraph>>> {
        assert!(self.iterating);
        assert!(universe.len() > 1);

        if self.exhausted {
            return Ok(None);
        }
        let Some(query_graph) = &mut self.query_graph else { panic!() };

        logger.log_words_state(query_graph);

        let this_bucket = resolve_query_graph(ctx, query_graph, universe)?;

        let child_query_graph = query_graph.clone();
        loop {
            if self.positions_to_remove.is_empty() {
                self.exhausted = true;
                break;
            } else {
                let position_to_remove = self.positions_to_remove.pop().unwrap();
                let did_delete_any_node = query_graph.remove_words_at_position(position_to_remove);
                if did_delete_any_node {
                    break;
                }
            }
        }

        Ok(Some(RankingRuleOutput { query: child_query_graph, candidates: this_bucket }))
    }

    fn end_iteration(
        &mut self,
        _ctx: &mut SearchContext<'search>,
        _logger: &mut dyn SearchLogger<QueryGraph>,
    ) {
        self.iterating = false;
        self.exhausted = true;
        self.positions_to_remove = vec![];
        self.query_graph = None;
    }
}