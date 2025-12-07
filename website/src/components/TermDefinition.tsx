import { useIsMobile } from "@/hooks/use-mobile";
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip";
import {
    Drawer,
    DrawerContent,
    DrawerDescription,
    DrawerHeader,
    DrawerTitle,
    DrawerTrigger,
} from "@/components/ui/drawer";
import { cn } from "@/lib/utils";

interface TermDefinitionProps {
    term: string;
    definition: string;
    className?: string;
}

export function TermDefinition({ term, definition, className }: TermDefinitionProps) {
    const isMobile = useIsMobile();

    const triggerClass = cn(
        "cursor-help border-b border-dashed border-muted-foreground/50 hover:border-primary transition-colors hover:text-primary",
        className
    );

    // Desktop: Tooltip
    if (!isMobile) {
        return (
            <TooltipProvider delayDuration={150}>
                <Tooltip>
                    <TooltipTrigger asChild>
                        <span className={triggerClass}>{term}</span>
                    </TooltipTrigger>
                    <TooltipContent className="max-w-[280px] p-4 text-sm bg-popover/95 backdrop-blur-sm border-primary/20 shadow-xl">
                        <p className="font-normal leading-relaxed">{definition}</p>
                    </TooltipContent>
                </Tooltip>
            </TooltipProvider>
        );
    }

    // Mobile: Drawer
    return (
        <Drawer>
            <DrawerTrigger asChild>
                <span className={triggerClass}>{term}</span>
            </DrawerTrigger>
            <DrawerContent>
                <DrawerHeader className="text-left pb-8 pt-6 px-6">
                    <DrawerTitle className="text-xl font-bold mb-3 flex items-center gap-2">
                        {term}
                    </DrawerTitle>
                    <DrawerDescription className="text-base text-foreground/90 leading-relaxed">
                        {definition}
                    </DrawerDescription>
                </DrawerHeader>
            </DrawerContent>
        </Drawer>
    );
}
